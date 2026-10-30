//! Authoritative text files in an existing, normal Sophia folder store.
//! No migration, cache rebuild, or generated-file writes happen here.
use super::{producer, CodeFile, FolderReport, MAX_SNAPSHOT_BYTES, STORE_PATH};
use moonkale_core::{
    async_trait, Applied, ContentRef, Node, NodeId, NodeKind, Op, OpResult, Query, QueryResult,
    Source, SourceDescriptor, SourceError, SourceId, SourceTruth, Transaction, Version,
};
use sophia_store::{sqlite::Sqlite, Store};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct StoreFolder {
    descriptor: SourceDescriptor,
    root: PathBuf,
}
fn error(e: impl std::fmt::Display) -> SourceError {
    SourceError::Invalid(e.to_string())
}
fn store_error(err: sophia_store::StoreError) -> SourceError {
    match err {
        sophia_store::StoreError::FileConflict { conflicts } => conflicts
            .first()
            .map(|(_, expected, found)| SourceError::Conflict {
                expected: Version(*expected),
                actual: Version(*found),
            })
            .unwrap_or_else(|| error("stale Sophia file tokens")),
        other => error(other),
    }
}
impl StoreFolder {
    /// Open only an existing store-truth folder. Missing/corrupt/derived stores
    /// refuse; there is never a fallback that writes generated files.
    pub fn open(folder: Arc<dyn Source>) -> Result<Self, SourceError> {
        let mut descriptor = folder.descriptor();
        let root = descriptor
            .id
            .as_str()
            .strip_prefix("folder:")
            .ok_or_else(|| error("not a local folder"))?;
        let root = PathBuf::from(root);
        descriptor.capabilities.write = true;
        descriptor.capabilities.watch = false;
        descriptor.capabilities.atomic_text = true;
        let this = Self { descriptor, root };
        this.store()?;
        Ok(this)
    }
    fn store(&self) -> Result<Store<Sqlite>, SourceError> {
        if moonkale_project_fs::truth::detect(&self.root).map_err(error)? != SourceTruth::Store {
            return Err(error("folder truth changed; reopen the folder"));
        }
        let dir = self.root.join(".moonkale");
        let path = self.root.join(STORE_PATH);
        if dir
            .symlink_metadata()
            .map_err(error)?
            .file_type()
            .is_symlink()
            || !path
                .symlink_metadata()
                .map_err(error)?
                .file_type()
                .is_file()
        {
            return Err(error(
                "authoritative store must be an existing regular file",
            ));
        }
        let store = Store::new(Sqlite::open(path).map_err(error)?);
        if store.is_derived().map_err(error)? {
            return Err(error(
                "store-truth marker points to a derived store; migrate explicitly",
            ));
        }
        Ok(store)
    }
    fn text(store: &Store<Sqlite>, path: &str) -> Result<(String, Version), SourceError> {
        let before = store.file_token(path).map_err(error)?;
        let manifest = store
            .file_manifest(path)
            .map_err(error)?
            .ok_or(SourceError::NotFound)?;
        let record = store
            .record(&manifest.source.ok_or(SourceError::NotFound)?)
            .map_err(error)?
            .ok_or(SourceError::NotFound)?;
        let text = String::from_utf8(record.bytes).map_err(error)?;
        let after = store.file_token(path).map_err(error)?;
        if before != after {
            return Err(SourceError::Conflict {
                expected: Version(before),
                actual: Version(after),
            });
        }
        Ok((text, Version(after)))
    }
    fn node(&self, path: &str, dir: bool, len: usize, version: Version) -> Node {
        let mut node = Node {
            id: NodeId::derive(&self.id(), path),
            source: self.id(),
            kind: if dir {
                NodeKind::Directory
            } else {
                NodeKind::File
            },
            label: if path.is_empty() {
                self.descriptor.display_name.clone()
            } else {
                path.rsplit('/').next().unwrap_or("").into()
            },
            native_key: path.into(),
            props: Default::default(),
            content: if dir {
                None
            } else {
                Some(ContentRef::Text {
                    len: len as u64,
                    lang: None,
                })
            },
            version,
        };
        let lang = node.language_hint().map(str::to_string);
        if let Some(ContentRef::Text { lang: hint, .. }) = &mut node.content {
            *hint = lang;
        }
        node
    }
    fn nodes(&self, store: &Store<Sqlite>) -> Result<Vec<Node>, SourceError> {
        let paths = store.files().map_err(error)?;
        if paths.len() > 100_000 {
            return Err(error("store folder exceeds file budget"));
        }
        let mut nodes = BTreeMap::new();
        nodes.insert(String::new(), self.node("", true, 0, Version(0)));
        let mut bytes = 0usize;
        for path in paths {
            if Path::new(&path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(error("invalid stored file path"));
            }
            let (text, version) = Self::text(store, &path)?;
            bytes = bytes.saturating_add(text.len());
            if text.len() > 512 * 1024 || bytes > MAX_SNAPSHOT_BYTES {
                return Err(error("store snapshot exceeds text budget"));
            }
            nodes.insert(path.clone(), self.node(&path, false, text.len(), version));
            let mut parent = Path::new(&path).parent();
            while let Some(p) = parent {
                let key = p.to_string_lossy().to_string();
                nodes
                    .entry(key.clone())
                    .or_insert_with(|| self.node(&key, true, 0, Version(0)));
                parent = p.parent();
            }
        }
        Ok(nodes.into_values().collect())
    }
    fn prepare(
        &self,
        store: &Store<Sqlite>,
        tx: Transaction,
    ) -> Result<Vec<(sophia_store::FileIngest, u64)>, SourceError> {
        tx.validate_text_proposal()?;
        if tx.ops.is_empty() || tx.ops.len() > 64 {
            return Err(error("proposal must contain 1..64 text edits"));
        }
        let nodes = self.nodes(store)?;
        let mut changes = BTreeMap::new();
        for op in tx.ops {
            let Op::WriteText {
                node,
                expected,
                patch,
            } = op
            else {
                return Err(SourceError::Unsupported(
                    "store folders accept versioned text proposals only".into(),
                ));
            };
            let n = nodes
                .iter()
                .find(|n| n.id == node && n.kind == NodeKind::File)
                .ok_or(SourceError::NotFound)?;
            let (text, actual) = Self::text(store, &n.native_key)?;
            if actual != expected {
                return Err(SourceError::Conflict { expected, actual });
            }
            let next = patch.apply(&text)?;
            if next.len() > 512 * 1024 {
                return Err(error("proposal file exceeds text budget"));
            }
            if changes
                .insert(n.native_key.clone(), (next, expected.0))
                .is_some()
            {
                return Err(error("duplicate proposal file"));
            }
        }
        let changed: BTreeSet<_> = changes.keys().cloned().collect();
        let mut files = Vec::new();
        let mut bytes = 0usize;
        for node in nodes.into_iter().filter(|n| n.kind == NodeKind::File) {
            let text = match changes.get(&node.native_key) {
                Some((text, _)) => text.clone(),
                None => Self::text(store, &node.native_key)?.0,
            };
            bytes = bytes.saturating_add(text.len());
            if bytes > MAX_SNAPSHOT_BYTES {
                return Err(error("store snapshot exceeds text budget"));
            }
            files.push(CodeFile { node, text });
        }
        let julia_changed = files
            .iter()
            .any(|f| changed.contains(&f.node.native_key) && f.language() == "julia");
        // Julia's module resolver may touch related definitions; require all
        // affected file tokens rather than author unreviewed companion files.
        // A derived cache may omit invalid definitions. An authoritative
        // store must refuse that loss before replacing any file's names.
        for file in files
            .iter()
            .filter(|f| changed.contains(&f.node.native_key))
        {
            let tree = moonkale_content_id::parse(
                file.language(),
                &file.text,
                &mut moonkale_content_id::Budget::new(),
            )
            .map_err(error)?;
            if tree.root_node().has_error() {
                return Err(error(format!(
                    "{}: proposed code has syntax errors",
                    file.node.native_key
                )));
            }
        }
        let mut report = FolderReport::default();
        let records = producer::produce(&self.root, &files, &changed, julia_changed, &mut report)?;
        if report.refused_definitions > 0 || !report.naming_diagnostics.is_empty() {
            return Err(error(
                "proposal cannot normalize every definition without loss",
            ));
        }
        if records.keys().any(|p| !changed.contains(p)) {
            return Err(error(
                "Include every affected Julia module file in the proposal",
            ));
        }
        Ok(records
            .into_iter()
            .map(|(path, record)| {
                let token = changes[&path].1;
                (record, token)
            })
            .collect())
    }
}
impl StoreFolder {
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(Self) -> Result<T, SourceError> + Send + 'static,
    ) -> Result<T, SourceError> {
        let source = self.clone();
        tokio::task::spawn_blocking(move || f(source))
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?
    }
    fn query_sync(&self, query: Query) -> Result<QueryResult, SourceError> {
        let nodes = self.nodes(&self.store()?)?;
        let parent = if let Query::Children(id) = &query {
            Some(*id)
        } else {
            None
        };
        let all = matches!(&query, Query::All { .. });
        let truncated = if let Query::All { limit, kinds } = &query {
            nodes
                .iter()
                .filter(|n| kinds.as_ref().is_none_or(|k| k.contains(&n.kind)))
                .count()
                > (*limit).min(100_000)
        } else {
            false
        };
        let selected: Vec<Node> = match query {
            Query::Text { dialect, text } if dialect == "path" => nodes
                .into_iter()
                .filter(|n| n.native_key == text.trim_start_matches("./"))
                .collect(),
            Query::Node(id) => nodes.into_iter().filter(|n| n.id == id).collect(),
            Query::Children(id) => {
                let parent = nodes
                    .iter()
                    .find(|n| n.id == id)
                    .ok_or(SourceError::NotFound)?
                    .native_key
                    .clone();
                nodes
                    .into_iter()
                    .filter(|n| {
                        !n.native_key.is_empty()
                            && Path::new(&n.native_key).parent().unwrap_or(Path::new(""))
                                == Path::new(&parent)
                    })
                    .collect()
            }
            Query::All { limit, kinds } => nodes
                .into_iter()
                .filter(|n| kinds.as_ref().is_none_or(|k| k.contains(&n.kind)))
                .take(limit.min(100_000))
                .collect(),
            _ => return Err(SourceError::Unsupported("store folder query".into())),
        };
        let ids: BTreeSet<_> = selected.iter().map(|n| n.id).collect();
        let edges = selected
            .iter()
            .filter_map(|node| {
                let from = if let Some(parent) = parent {
                    Some(parent)
                } else if all && !node.native_key.is_empty() {
                    let path = Path::new(&node.native_key).parent()?.to_string_lossy();
                    let id = NodeId::derive(&self.id(), &path);
                    ids.contains(&id).then_some(id)
                } else {
                    None
                }?;
                Some(moonkale_core::Edge::contains(&self.id(), from, node.id))
            })
            .collect();
        Ok(QueryResult {
            nodes: selected,
            edges,
            truncated,
            table: None,
        })
    }
    fn fetch_text_sync(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        let store = self.store()?;
        let paths = store.files().map_err(error)?;
        let path = paths
            .iter()
            .find(|p| NodeId::derive(&self.id(), p) == node)
            .ok_or(SourceError::NotFound)?;
        Self::text(&store, path)
    }
    fn preview_text_sync(&self, tx: Transaction) -> Result<(), SourceError> {
        let store = self.store()?;
        let files = self.prepare(&store, tx)?;
        let proposal = store.propose_files(&files).map_err(error)?;
        if let Some(e) = proposal.error {
            return Err(store_error(e));
        }
        if let Some((_, expected, found)) = proposal.conflicts.first() {
            return Err(SourceError::Conflict {
                expected: Version(*expected),
                actual: Version(*found),
            });
        }
        Ok(())
    }
    fn apply_sync(&self, tx: Transaction) -> Result<Applied, SourceError> {
        let store = self.store()?;
        let files = self.prepare(&store, tx)?;
        // Sophia validates every token and writes every selected file in one
        // database transaction. No generated filesystem text is modified.
        store.replace_files_checked(&files).map_err(store_error)?;
        Ok(Applied {
            results: files
                .iter()
                .map(|(file, _)| {
                    let (current, version) = Self::text(&store, &file.path)?;
                    if file.text.as_deref() != Some(current.as_str()) {
                        return Err(error(
                            "store changed again after the committed proposal; reopen to inspect",
                        ));
                    }
                    Ok(OpResult::Ok {
                        node: NodeId::derive(&self.id(), &file.path),
                        version,
                        warnings: Vec::new(),
                    })
                })
                .collect::<Result<Vec<_>, SourceError>>()?,
        })
    }
}
#[async_trait]
impl Source for StoreFolder {
    fn id(&self) -> SourceId {
        self.descriptor.id.clone()
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        self.call(move |source| source.query_sync(query)).await
    }
    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        self.call(move |source| source.fetch_text_sync(node)).await
    }
    async fn preview_text(&self, tx: Transaction) -> Result<(), SourceError> {
        self.call(move |source| source.preview_text_sync(tx)).await
    }
    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        self.call(move |source| source.apply_sync(tx)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::TextPatch;
    fn fixture() -> (tempfile::TempDir, StoreFolder) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".moonkale")).unwrap();
        std::fs::write(dir.path().join("a.py"), "def a():\n    return 1\n").unwrap();
        std::fs::write(dir.path().join("b.py"), "def b():\n    return 2\n").unwrap();
        let folder = Arc::new(moonkale_project_fs::FolderSource::open(dir.path()).unwrap());
        let draft = StoreFolder {
            descriptor: folder.descriptor(),
            root: dir.path().to_path_buf(),
        };
        let files: Vec<_> = [
            ("a.py", "def a():\n    return 1\n"),
            ("b.py", "def b():\n    return 2\n"),
        ]
        .into_iter()
        .map(|(p, t)| CodeFile {
            node: draft.node(p, false, t.len(), Version(0)),
            text: t.into(),
        })
        .collect();
        let changed = files.iter().map(|f| f.node.native_key.clone()).collect();
        let records = producer::produce(
            dir.path(),
            &files,
            &changed,
            false,
            &mut FolderReport::default(),
        )
        .unwrap();
        let store = Store::new(Sqlite::open(dir.path().join(STORE_PATH)).unwrap());
        store
            .replace_files_checked(&records.into_values().map(|f| (f, 0)).collect::<Vec<_>>())
            .unwrap();
        std::fs::write(
            dir.path().join(".moonkale/truth.json"),
            "{\"truth\":\"store\"}",
        )
        .unwrap();
        let source = StoreFolder::open(Arc::new(
            moonkale_project_fs::FolderSource::open(dir.path()).unwrap(),
        ))
        .unwrap();
        (dir, source)
    }
    async fn tx(source: &StoreFolder) -> Transaction {
        let mut ops = Vec::new();
        for (path, next) in [
            ("a.py", "def a():\n    return 10\n"),
            ("b.py", "def b():\n    return 20\n"),
        ] {
            let node = NodeId::derive(&source.id(), path);
            let (text, expected) = source.fetch_text(node).await.unwrap();
            ops.push(Op::WriteText {
                node,
                expected,
                patch: TextPatch::whole(next, text.chars().count()),
            });
        }
        Transaction { ops }
    }
    #[tokio::test]
    async fn preview_is_nonmutating_and_two_file_apply_is_one_checked_transaction() {
        let (dir, source) = fixture();
        let tx = tx(&source).await;
        let children = source
            .query(Query::Children(source.descriptor().root))
            .await
            .unwrap();
        assert_eq!(children.nodes.len(), 2);
        assert_eq!(children.edges.len(), 2);
        let capped = source
            .query(Query::All {
                limit: 1,
                kinds: None,
            })
            .await
            .unwrap();
        assert_eq!(capped.nodes.len(), 1);
        assert!(capped.truncated);
        let before = source.store().unwrap().last_seq().unwrap();
        source.preview_text(tx.clone()).await.unwrap();
        assert_eq!(source.store().unwrap().last_seq().unwrap(), before);
        assert_eq!(source.apply(tx).await.unwrap().results.len(), 2);
        assert!(source
            .fetch_text(NodeId::derive(&source.id(), "a.py"))
            .await
            .unwrap()
            .0
            .contains("10"));
        assert!(source
            .fetch_text(NodeId::derive(&source.id(), "b.py"))
            .await
            .unwrap()
            .0
            .contains("20"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.py")).unwrap(),
            "def a():\n    return 1\n"
        );
    }
    #[tokio::test]
    async fn one_stale_file_refuses_every_write_and_partial_selection_can_apply() {
        let (_dir, source) = fixture();
        let mut old = tx(&source).await;
        let second = old.ops.pop().unwrap();
        source
            .apply(Transaction { ops: vec![second] })
            .await
            .unwrap();
        let before = source.store().unwrap().last_seq().unwrap();
        let first = old.ops[0].clone();
        old.ops.push(match tx(&source).await.ops.pop().unwrap() {
            Op::WriteText { node, patch, .. } => Op::WriteText {
                node,
                expected: Version(0),
                patch,
            },
            _ => unreachable!(),
        });
        assert!(matches!(
            source.apply(old).await,
            Err(SourceError::Conflict { .. })
        ));
        assert_eq!(source.store().unwrap().last_seq().unwrap(), before);
        assert!(source
            .fetch_text(NodeId::derive(&source.id(), "a.py"))
            .await
            .unwrap()
            .0
            .contains("return 1\n"));
        source
            .apply(Transaction { ops: vec![first] })
            .await
            .unwrap();
        assert!(source
            .fetch_text(NodeId::derive(&source.id(), "a.py"))
            .await
            .unwrap()
            .0
            .contains("return 10\n"));
    }
    #[tokio::test]
    async fn syntax_errors_in_second_file_do_not_remove_authored_names() {
        let (_dir, source) = fixture();
        let mut batch = tx(&source).await;
        let second = NodeId::derive(&source.id(), "b.py");
        let (old, expected) = source.fetch_text(second).await.unwrap();
        batch.ops[1] = Op::WriteText {
            node: second,
            expected,
            patch: TextPatch::whole("def b(:\n", old.chars().count()),
        };
        let before = source.store().unwrap().last_seq().unwrap();
        assert!(source.preview_text(batch.clone()).await.is_err());
        assert!(source.apply(batch).await.is_err());
        assert_eq!(source.store().unwrap().last_seq().unwrap(), before);
        assert!(source
            .fetch_text(NodeId::derive(&source.id(), "a.py"))
            .await
            .unwrap()
            .0
            .contains("return 1\n"));
        assert_eq!(source.fetch_text(second).await.unwrap().0, old);
    }
    #[tokio::test]
    async fn invalid_second_patch_never_partially_applies_first_and_structural_ops_refuse() {
        let (_dir, source) = fixture();
        let mut batch = tx(&source).await;
        if let Op::WriteText { patch, .. } = &mut batch.ops[1] {
            *patch = TextPatch::whole("bad", usize::MAX);
        }
        assert!(source.apply(batch).await.is_err());
        assert!(source
            .fetch_text(NodeId::derive(&source.id(), "a.py"))
            .await
            .unwrap()
            .0
            .contains("return 1\n"));
        assert!(source
            .apply(Transaction::delete(NodeId::derive(&source.id(), "a.py")))
            .await
            .is_err());
    }
}
