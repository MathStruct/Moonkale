//! The `Source` impl over a local folder.
//!
//! - Ids: `NodeId::derive(source_id, relative_path)`, the root is `""`.
//! - Query nodes carry cheap metadata change hints, never save preconditions.
//!   Fetches return exact-byte content versions used by optimistic saves.
//! - Saves lock an open handle, validate its bytes, stage a unique sibling,
//!   and revalidate the handle and path immediately before replacement.
//! - A `native_key ↔ NodeId` map is filled as nodes are listed, so `fetch`
//!   and `apply` on an id we handed out never need a directory walk. Ids for
//!   paths we have not listed yet are still resolvable through the map after
//!   the parent is listed, which is the only order the UI produces.

use crate::tree;
use moonkale_core::source::transaction::OpResult;
use moonkale_core::{
    async_trait, Applied, Capabilities, ContentRef, Edge, Node, NodeId, NodeKind, Op, Query,
    QueryResult, Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Version,
};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::UNIX_EPOCH;

pub struct FolderSource {
    id: SourceId,
    root: PathBuf,
    /// `NodeId → relative path` for everything we have listed so far.
    known: RwLock<HashMap<NodeId, String>>,
    /// The watcher, started by the first `changes_since` (Milestone 16) —
    /// only folders somebody follows are watched. `None` inside: it could
    /// not start (e.g. the inotify limit); the source then reports itself
    /// as not watched.
    watch: tokio::sync::OnceCell<Option<crate::watch::FolderWatch>>,
}

/// How long one `changes_since` call waits when nothing happens. Below the
/// usual 30–60 s idle timeouts of proxies in front of a server.
const LONG_POLL: std::time::Duration = std::time::Duration::from_secs(25);

impl FolderSource {
    /// Open `root` (must be an existing directory). The source id is
    /// `folder:<canonical path>` so the same folder always gets the same ids.
    pub fn open(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = std::fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "not a directory",
            ));
        }
        let id = SourceId::new(format!("folder:{}", root.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Ok(Self {
            id,
            root,
            known: RwLock::new(known),
            watch: tokio::sync::OnceCell::new(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_id(&self) -> NodeId {
        NodeId::derive(&self.id, "")
    }

    fn node_id(&self, rel: &str) -> NodeId {
        let id = NodeId::derive(&self.id, rel);
        self.known
            .write()
            .unwrap()
            .entry(id)
            .or_insert_with(|| rel.to_string());
        id
    }

    /// Forget the ids of `rel` and everything under it (#15): ids derive from
    /// paths, so a stale entry would point a later file at the same path to
    /// whoever still holds the old id.
    fn forget(&self, rel: &str) {
        let under = format!("{rel}/");
        self.known
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, r| r != rel && !r.starts_with(&under));
    }

    /// The absolute path of `rel`, kept inside the folder (#5).
    async fn path(&self, rel: &str) -> Result<PathBuf, SourceError> {
        tree::jailed(&self.root, rel).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                SourceError::Invalid(e.to_string())
            } else {
                e.into()
            }
        })
    }

    /// [`Self::path`] for rename and delete: the entry itself (a link, not
    /// its target).
    async fn entry_path(&self, rel: &str) -> Result<PathBuf, SourceError> {
        tree::jailed_entry(&self.root, rel)
            .await
            .map_err(|e| SourceError::Invalid(e.to_string()))
    }

    fn rel_of(&self, id: NodeId) -> Result<String, SourceError> {
        self.known
            .read()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(SourceError::NotFound)
    }

    fn node_for(&self, rel: &str, is_dir: bool, len: u64, version: Version) -> Node {
        let name = rel.rsplit('/').next().unwrap_or("").to_string();
        let label = if rel.is_empty() {
            self.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into())
        } else {
            name
        };
        let content = if is_dir {
            None
        } else if looks_textual(rel) {
            Some(ContentRef::Text { len, lang: None })
        } else {
            Some(ContentRef::Blob {
                len,
                mime: mime_of(rel).map(str::to_string),
            })
        };
        let mut node = Node {
            props: Default::default(),
            id: self.node_id(rel),
            source: self.id.clone(),
            kind: if is_dir {
                NodeKind::Directory
            } else {
                NodeKind::File
            },
            label,
            native_key: rel.to_string(),
            content,
            version,
        };
        let hint = node.language_hint().map(str::to_string);
        if let Some(ContentRef::Text { lang, .. }) = &mut node.content {
            *lang = hint;
        }
        node
    }

    /// A cheap listing hint only. Fetch a content version before saving.
    fn version_of(meta: &std::fs::Metadata) -> Version {
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        mtime.hash(&mut h);
        meta.len().hash(&mut h);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            meta.dev().hash(&mut h);
            meta.ino().hash(&mut h);
            meta.ctime().hash(&mut h);
            meta.ctime_nsec().hash(&mut h);
        }
        Version(h.finish())
    }

    async fn stat(&self, rel: &str) -> Result<(std::fs::Metadata, Version), SourceError> {
        let meta = tokio::fs::metadata(self.path(rel).await?).await?;
        let hint = Self::version_of(&meta);
        Ok((meta, hint))
    }
}

/// Heuristic until the index knows better: anything with a known text
/// extension or no extension at all is offered as text.
/// A MIME type for the blobs Moonkale can show (images, spec 008).
fn mime_of(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

fn looks_textual(rel: &str) -> bool {
    const BINARY: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "zip", "gz", "tar", "wasm", "so", "dll",
        "dylib", "o", "a", "class", "jar", "sqlite", "db", "duckdb", "ttf", "otf", "woff", "woff2",
        "mp3", "mp4", "mov", "svg", "bmp", "avif",
    ];
    match rel.rsplit('.').next() {
        Some(ext) if rel.contains('.') => !BINARY.contains(&ext.to_ascii_lowercase().as_str()),
        _ => true,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for FolderSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            truth: Default::default(),
            id: self.id.clone(),
            display_name: self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.root.display().to_string()),
            family: SourceFamily::Folder,
            capabilities: Capabilities {
                read: true,
                write: true,
                watch: true,
                text_query: None,
                extra_text_query: None,
            },
            root: self.root_id(),
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Neighbours { node, depth, direction } => {
                self.neighbours(node, depth, direction).await
            }
            // `path`: resolve a relative path directly (hidden files too, e.g.
            // `.moonkale/settings.json`), without walking the tree.
            Query::Text { dialect, text } if dialect == "path" => {
                let rel = text.trim().trim_start_matches("./").trim_matches('/').to_string();
                if let Err(e) = tree::check_rel(&rel) {
                    return Err(SourceError::Invalid(e));
                }
                let (meta, v) = self.stat(&rel).await?;
                Ok(QueryResult::single(self.node_for(
                    &rel,
                    meta.is_dir(),
                    meta.len(),
                    v,
                )))
            }
            // `ls`: the entries of a directory by relative path, hidden ones
            // included and nothing ignored (Milestone 15: the saved agent
            // sessions under `.moonkale/`, which `Children` never lists).
            Query::Text { dialect, text } if dialect == "ls" => {
                let rel = text.trim().trim_start_matches("./").trim_matches('/').to_string();
                if let Err(e) = tree::check_rel(&rel) {
                    return Err(SourceError::Invalid(e));
                }
                let dir = self.path(&rel).await?;
                let mut result = QueryResult::default();
                let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
                    return Ok(result);
                };
                let parent = self.node_id(&rel);
                let mut listed = Vec::new();
                while let Ok(Some(e)) = entries.next_entry().await {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let child_rel = if rel.is_empty() {
                        name
                    } else {
                        format!("{rel}/{name}")
                    };
                    if let Ok(meta) = e.metadata().await {
                        listed.push((child_rel, meta));
                    }
                }
                listed.sort_by(|a, b| a.0.cmp(&b.0));
                for (child_rel, meta) in listed {
                    let version = self.stat(&child_rel).await.map(|(_, v)| v).unwrap_or_default();
                    let node = self.node_for(&child_rel, meta.is_dir(), meta.len(), version);
                    result.edges.push(Edge::contains(&self.id, parent, node.id));
                    result.nodes.push(node);
                }
                Ok(result)
            }
            Query::All { .. } | Query::Text { .. } => Err(SourceError::Unsupported(
                "folders answer Node/Children/Neighbours and Text{path|ls}; the index has the whole graph"
                    .into(),
            )),
            Query::Node(id) => {
                let rel = self.rel_of(id)?;
                let (meta, v) = self.stat(&rel).await?;
                Ok(QueryResult::single(self.node_for(
                    &rel,
                    meta.is_dir(),
                    meta.len(),
                    v,
                )))
            }
            Query::Children(id) => {
                let rel = self.rel_of(id)?;
                let dir = self.path(&rel).await?;
                let root = self.root.clone();
                let entries = tokio::task::spawn_blocking(move || tree::list_children(&root, &dir))
                    .await
                    .map_err(|e| SourceError::Io(e.to_string()))??;
                let mut result = QueryResult::default();
                for e in entries {
                    let version = self.stat(&e.rel).await.map(|(_, v)| v).unwrap_or_default();
                    let node = self.node_for(&e.rel, e.is_dir, e.len, version);
                    result.edges.push(Edge::contains(&self.id, id, node.id));
                    result.nodes.push(node);
                }
                Ok(result)
            }
        }
    }

    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        let rel = self.rel_of(node)?;
        let path = self.path(&rel).await?;
        let (bytes, version) = read_snapshot(path).await?;
        let text = String::from_utf8(bytes)
            .map_err(|_| SourceError::Unsupported("not valid UTF-8".into()))?;
        Ok((text, version))
    }

    async fn fetch_bytes(&self, node: NodeId) -> Result<(Vec<u8>, Version), SourceError> {
        let rel = self.rel_of(node)?;
        read_snapshot(self.path(&rel).await?).await
    }

    async fn apply(&self, tx: moonkale_core::Transaction) -> Result<Applied, SourceError> {
        let mut applied = Applied::default();
        for op in tx.ops {
            match op {
                Op::WriteText {
                    node,
                    expected,
                    patch,
                } => applied
                    .results
                    .push(match self.write_text(node, expected, &patch).await {
                        Ok(version) => OpResult::Ok { node, version },
                        Err(error) => OpResult::Refused { node, error },
                    }),
                Op::CreateText { parent, name, text } => {
                    applied
                        .results
                        .push(match self.create_text(parent, &name, &text).await {
                            Ok((node, version)) => OpResult::Ok { node, version },
                            Err(error) => OpResult::Refused {
                                node: parent,
                                error,
                            },
                        })
                }
                Op::CreateDir { parent, name } => {
                    applied
                        .results
                        .push(match self.create_dir(parent, &name).await {
                            Ok((node, version)) => OpResult::Ok { node, version },
                            Err(error) => OpResult::Refused {
                                node: parent,
                                error,
                            },
                        })
                }
                Op::Rename { node, to } => {
                    applied.results.push(match self.rename(node, &to).await {
                        Ok((new_node, version)) => OpResult::Ok {
                            node: new_node,
                            version,
                        },
                        Err(error) => OpResult::Refused { node, error },
                    })
                }
                Op::Delete { node } => applied.results.push(match self.delete(node).await {
                    Ok(()) => OpResult::Ok {
                        node,
                        version: Version(0),
                    },
                    Err(error) => OpResult::Refused { node, error },
                }),
            }
        }
        Ok(applied)
    }

    async fn changes_since(
        &self,
        since: u64,
    ) -> Result<Option<moonkale_core::Changes>, SourceError> {
        let watch = self
            .watch
            .get_or_init(|| async {
                let root = self.root.clone();
                match tokio::task::spawn_blocking(move || crate::watch::FolderWatch::start(&root))
                    .await
                {
                    Ok(Ok(w)) => Some(w),
                    Ok(Err(e)) => {
                        eprintln!("moonkale: cannot watch {}: {e}", self.root.display());
                        None
                    }
                    Err(_) => None,
                }
            })
            .await;
        match watch {
            Some(w) => Ok(Some(w.changes_since(since, LONG_POLL).await)),
            None => Ok(None),
        }
    }
}

impl FolderSource {
    async fn neighbours(
        &self,
        start: NodeId,
        depth: u32,
        direction: moonkale_core::Direction,
    ) -> Result<QueryResult, SourceError> {
        use moonkale_core::Direction;
        use std::collections::{HashSet, VecDeque};
        const MAX_NODES: usize = 4096;
        const MAX_DEPTH: u32 = 64;
        let first = self.query(Query::Node(start)).await?.nodes.remove(0);
        let mut result = QueryResult::single(first);
        let mut seen = HashSet::from([start]);
        let mut queue = VecDeque::from([(start, 0)]);
        while let Some((id, hops)) = queue.pop_front() {
            if hops >= depth {
                continue;
            }
            if hops >= MAX_DEPTH {
                result.truncated = true;
                continue;
            }
            let rel = self.rel_of(id)?;
            let mut adjacent = Vec::new();
            if direction != Direction::In
                && result
                    .nodes
                    .iter()
                    .any(|n| n.id == id && n.kind == NodeKind::Directory)
            {
                let root = self.root.clone();
                let dir = self.path(&rel).await?;
                let (entries, truncated) = tokio::task::spawn_blocking(move || {
                    tree::list_children_bounded(&root, &dir, MAX_NODES)
                })
                .await
                .map_err(|e| SourceError::Io(e.to_string()))??;
                result.truncated |= truncated;
                for entry in entries {
                    let version = self
                        .stat(&entry.rel)
                        .await
                        .map(|(_, v)| v)
                        .unwrap_or_default();
                    let n = self.node_for(&entry.rel, entry.is_dir, entry.len, version);
                    adjacent.push((Edge::contains(&self.id, id, n.id), n));
                }
            }
            if direction != Direction::Out && !rel.is_empty() {
                let parent = rel.rsplit_once('/').map_or("", |(p, _)| p);
                let parent_id = self.node_id(parent);
                let parent_node = self.query(Query::Node(parent_id)).await?.nodes.remove(0);
                adjacent.push((Edge::contains(&self.id, parent_id, id), parent_node));
            }
            for (edge, node) in adjacent {
                if !seen.contains(&node.id) {
                    if seen.len() == MAX_NODES {
                        result.truncated = true;
                        continue;
                    }
                    seen.insert(node.id);
                    queue.push_back((node.id, hops + 1));
                    result.nodes.push(node);
                }
                if !result.edges.contains(&edge) {
                    result.edges.push(edge);
                }
            }
        }
        // Include containment edges among all reached nodes, even at the boundary.
        for node in &result.nodes {
            if node.native_key.is_empty() {
                continue;
            }
            let parent = node.native_key.rsplit_once('/').map_or("", |(p, _)| p);
            let parent = NodeId::derive(&self.id, parent);
            if seen.contains(&parent) {
                let edge = Edge::contains(&self.id, parent, node.id);
                if !result.edges.contains(&edge) {
                    result.edges.push(edge);
                }
            }
        }
        Ok(result)
    }

    /// Create `name` under the directory `parent`; refuses to overwrite.
    async fn create_text(
        &self,
        parent: NodeId,
        name: &str,
        text: &str,
    ) -> Result<(NodeId, Version), SourceError> {
        let parent_rel = self.rel_of(parent)?;
        let name = name.trim_matches('/');
        if name.is_empty() || tree::check_rel(name).is_err() {
            return Err(SourceError::Invalid(format!("bad name {name:?}")));
        }
        let rel = if parent_rel.is_empty() {
            name.to_string()
        } else {
            format!("{parent_rel}/{name}")
        };
        let path = self.path(&rel).await?;
        if tokio::fs::symlink_metadata(&path).await.is_ok() {
            return Err(SourceError::Invalid(format!("{rel} already exists")));
        }
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        // `create_new`: a file that appeared since the check is refused, not
        // truncated (#15).
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
            .map_err(|e| exists_as_invalid(e, &rel))?;
        write_all(file, text.as_bytes()).await?;
        Ok((self.node_id(&rel), bytes_version(text.as_bytes())))
    }

    /// `parent/name` as a checked relative path (no `..`, not empty).
    fn child_rel(&self, parent: NodeId, name: &str) -> Result<String, SourceError> {
        let parent_rel = self.rel_of(parent)?;
        let name = name.trim_matches('/');
        if name.is_empty() || tree::check_rel(name).is_err() {
            return Err(SourceError::Invalid(format!("bad name {name:?}")));
        }
        Ok(if parent_rel.is_empty() {
            name.to_string()
        } else {
            format!("{parent_rel}/{name}")
        })
    }

    async fn create_dir(
        &self,
        parent: NodeId,
        name: &str,
    ) -> Result<(NodeId, Version), SourceError> {
        let rel = self.child_rel(parent, name)?;
        let path = self.path(&rel).await?;
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        // `create_dir`, not `create_dir_all`: an existing one is an error.
        tokio::fs::create_dir(&path)
            .await
            .map_err(|e| exists_as_invalid(e, &rel))?;
        let (_, version) = self.stat(&rel).await?;
        Ok((self.node_id(&rel), version))
    }

    /// Rename/move within the folder. The new node id derives from the new
    /// path; the old id is forgotten.
    async fn rename(&self, node: NodeId, to: &str) -> Result<(NodeId, Version), SourceError> {
        let from = self.rel_of(node)?;
        if from.is_empty() {
            return Err(SourceError::Invalid("cannot rename the root".into()));
        }
        let to = to.trim_matches('/');
        if to.is_empty() || tree::check_rel(to).is_err() {
            return Err(SourceError::Invalid(format!("bad path {to:?}")));
        }
        if to == from {
            let (_, version) = self.stat(&from).await?;
            return Ok((node, version));
        }
        let src = self.entry_path(&from).await?;
        let dst = self.path(to).await?;
        if to.starts_with(&format!("{from}/")) {
            return Err(SourceError::Invalid(format!(
                "cannot move {from} into itself"
            )));
        }
        if tokio::fs::symlink_metadata(&dst).await.is_ok() {
            return Err(SourceError::Invalid(format!("{to} already exists")));
        }
        // Parents are made only after the checks, and removed again if the
        // rename fails (#15).
        let made = make_parents(&dst).await?;
        let moved = {
            let (src, dst) = (src.clone(), dst.clone());
            tokio::task::spawn_blocking(move || rename_no_replace(&src, &dst))
                .await
                .map_err(|e| SourceError::Io(e.to_string()))?
        };
        if let Err(e) = moved {
            remove_empty_parents(&dst, made).await;
            return Err(exists_as_invalid(e, to));
        }
        self.forget(&from);
        // A link leading outside was moved, but is not read through (#5).
        let version = self.stat(to).await.map(|(_, v)| v).unwrap_or_default();
        Ok((self.node_id(to), version))
    }

    /// Delete = move into `.moonkale/trash/<unix-ms>/<path>` so a slip can be
    /// undone by hand; the trash is git-ignored like the rest of `.moonkale`.
    async fn delete(&self, node: NodeId) -> Result<(), SourceError> {
        let rel = self.rel_of(node)?;
        if rel.is_empty() {
            return Err(SourceError::Invalid("cannot delete the root".into()));
        }
        let src = self.entry_path(&rel).await?;
        tokio::fs::symlink_metadata(&src).await?;
        let trash = self.root.join(".moonkale").join("trash");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        // Never over an earlier trash copy (#15): two deletes of the same
        // path in one millisecond get `<ms>`, `<ms>-1`, …
        let mut n = 0u32;
        loop {
            let slot = if n == 0 {
                stamp.to_string()
            } else {
                format!("{stamp}-{n}")
            };
            let dst = trash.join(slot).join(&rel);
            let made = make_parents(&dst).await?;
            let moved = {
                let (src, dst) = (src.clone(), dst.clone());
                tokio::task::spawn_blocking(move || rename_no_replace(&src, &dst))
                    .await
                    .map_err(|e| SourceError::Io(e.to_string()))?
            };
            match moved {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => {
                    remove_empty_parents(&dst, made).await;
                    n += 1;
                }
                Err(e) => {
                    remove_empty_parents(&dst, made).await;
                    return Err(e.into());
                }
            }
        }
        self.forget(&rel);
        prune_trash(&trash, stamp).await;
        Ok(())
    }

    async fn write_text(
        &self,
        node: NodeId,
        expected: Version,
        patch: &moonkale_core::TextPatch,
    ) -> Result<Version, SourceError> {
        let rel = self.rel_of(node)?;
        let path = self.path(&rel).await?;
        let patch = patch.clone();
        tokio::task::spawn_blocking(move || save_text(&path, expected, &patch))
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?
    }
}

/// Trash folders older than this are removed on the next delete.
const TRASH_DAYS: u128 = 30;

fn exists_as_invalid(e: std::io::Error, rel: &str) -> SourceError {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        SourceError::Invalid(format!("{rel} already exists"))
    } else {
        e.into()
    }
}

async fn write_all(mut file: tokio::fs::File, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    file.write_all(bytes).await?;
    file.flush().await
}

/// A temp file next to `path` that no other save uses.
fn unique_tmp(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.with_file_name(format!(
        ".{name}.{}-{nanos}-{}.moonkale-tmp",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Atomic exclusion on Linux/Android, macOS and Windows. Only platforms
/// without a native primitive use the check-then-rename fallback.
fn rename_no_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use rustix::fs::{renameat_with, RenameFlags, CWD};
        // Unsupported filesystems must refuse: falling back would reintroduce
        // the destination replacement race.
        renameat_with(CWD, src, CWD, dst, RenameFlags::NOREPLACE)?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        unsafe extern "C" {
            fn renamex_np(
                from: *const std::ffi::c_char,
                to: *const std::ffi::c_char,
                flags: u32,
            ) -> std::ffi::c_int;
        }
        let src = CString::new(src.as_os_str().as_bytes())?;
        let dst = CString::new(dst.as_os_str().as_bytes())?;
        // Darwin sys/stdio.h: RENAME_EXCL = 0x00000004.
        // SAFETY: both pointers are valid NUL-terminated paths for this call.
        if unsafe { renamex_np(src.as_ptr(), dst.as_ptr(), 4) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        fn wide(path: &Path) -> std::io::Result<Vec<u16>> {
            let mut path: Vec<_> = path.as_os_str().encode_wide().collect();
            if path.contains(&0) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "NUL in path",
                ));
            }
            path.push(0);
            Ok(path)
        }
        let src = wide(src)?;
        let dst = wide(dst)?;
        // No MOVEFILE_REPLACE_EXISTING, nor cross-volume copy fallback.
        // SAFETY: both UTF-16 paths are NUL-terminated and live through the call.
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), 0) } != 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "windows"
    )))]
    {
        if std::fs::symlink_metadata(dst).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "destination exists",
            ));
        }
        std::fs::rename(src, dst)
    }
}

/// Create `path`'s missing parents; returns how many levels were made.
async fn make_parents(path: &Path) -> std::io::Result<usize> {
    let mut missing = 0;
    let mut dir = path.parent();
    while let Some(d) = dir {
        if tokio::fs::symlink_metadata(d).await.is_ok() {
            break;
        }
        missing += 1;
        dir = d.parent();
    }
    if let Some(d) = path.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    Ok(missing)
}

/// Undo [`make_parents`]: remove the `levels` directories it made, as long
/// as they are still empty.
async fn remove_empty_parents(path: &Path, levels: usize) {
    let mut dir = path.parent();
    for _ in 0..levels {
        let Some(d) = dir else { break };
        if tokio::fs::remove_dir(d).await.is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Remove trash folders (`<ms>` or `<ms>-<n>`) older than [`TRASH_DAYS`].
/// Best effort: a failure leaves the folder for next time.
async fn prune_trash(trash: &Path, now_ms: u128) {
    let Ok(mut entries) = tokio::fs::read_dir(trash).await else {
        return;
    };
    let max_age = TRASH_DAYS * 24 * 60 * 60 * 1000;
    while let Ok(Some(e)) = entries.next_entry().await {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ms) = name.split('-').next().unwrap_or("").parse::<u128>() else {
            continue;
        };
        if now_ms.saturating_sub(ms) > max_age {
            let _ = tokio::fs::remove_dir_all(e.path()).await;
        }
    }
}

/// The largest file `fetch_text`/`fetch_bytes` read whole (#14): a multi-GB
/// file must not be one click or one agent call away from filling memory.
pub const MAX_FETCH: u64 = 64 << 20;

fn too_big(meta: &std::fs::Metadata) -> Result<(), SourceError> {
    if meta.len() > MAX_FETCH {
        return Err(SourceError::Unsupported(format!(
            "too large to open ({} MB; the limit is {} MB)",
            meta.len() >> 20,
            MAX_FETCH >> 20
        )));
    }
    Ok(())
}

// All read-only content opens pass this seam so listing regressions can count
// opens for one fixture without interfering with concurrently running tests.
fn open_content(path: impl AsRef<Path>) -> std::io::Result<std::fs::File> {
    #[cfg(test)]
    if let Some(count) = CONTENT_OPENS.lock().unwrap().get_mut(path.as_ref()) {
        *count += 1;
    }
    std::fs::File::open(path)
}

#[cfg(test)]
static CONTENT_OPENS: std::sync::LazyLock<std::sync::Mutex<HashMap<PathBuf, usize>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn digest_version(hash: blake3::Hash) -> Version {
    Version(u64::from_le_bytes(
        hash.as_bytes()[..8].try_into().expect("eight digest bytes"),
    ))
}

fn bytes_version(bytes: &[u8]) -> Version {
    digest_version(blake3::hash(bytes))
}

fn regular_file(meta: &std::fs::Metadata) -> Result<(), SourceError> {
    if !meta.is_file() {
        return Err(SourceError::Unsupported(
            "content requires a regular file".into(),
        ));
    }
    Ok(())
}

fn snapshot(file: &mut std::fs::File) -> Result<(Vec<u8>, Version), SourceError> {
    use std::io::{Read, Seek};
    let meta = file.metadata()?;
    regular_file(&meta)?;
    too_big(&meta)?;
    file.rewind()?;
    let mut bytes = Vec::new();
    file.take(MAX_FETCH + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FETCH {
        return Err(SourceError::Unsupported(
            "file grew past the fetch limit".into(),
        ));
    }
    let version = bytes_version(&bytes);
    Ok((bytes, version))
}

async fn read_snapshot(path: PathBuf) -> Result<(Vec<u8>, Version), SourceError> {
    tokio::task::spawn_blocking(move || {
        regular_file(&std::fs::metadata(&path)?)?;
        snapshot(&mut open_content(path)?)
    })
    .await
    .map_err(|e| SourceError::Io(e.to_string()))?
}

fn save_text(
    path: &Path,
    expected: Version,
    patch: &moonkale_core::TextPatch,
) -> Result<Version, SourceError> {
    save_text_checked(path, expected, patch, || {})
}

fn save_text_checked(
    path: &Path,
    expected: Version,
    patch: &moonkale_core::TextPatch,
    staged_hook: impl FnOnce(),
) -> Result<Version, SourceError> {
    use std::io::Write;
    regular_file(&std::fs::metadata(path)?)?;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    // A competing source/process must refuse rather than stage a stale save.
    file.try_lock()
        .map_err(|_| SourceError::Unsupported("file is locked by another writer".into()))?;
    let identity = same_file::Handle::from_file(file.try_clone()?)?;
    let (bytes, actual) = snapshot(&mut file)?;
    if actual != expected {
        return Err(SourceError::Conflict { expected, actual });
    }
    let current =
        String::from_utf8(bytes).map_err(|_| SourceError::Unsupported("not valid UTF-8".into()))?;
    let next = patch.apply(&current)?;
    let tmp = unique_tmp(path);
    let mut created = false;
    let result = (|| {
        let mut staged = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        created = true;
        staged.write_all(next.as_bytes())?;
        staged.flush()?;
        staged_hook();
        let (_, actual) = snapshot(&mut file)?;
        if actual != expected {
            return Err(SourceError::Conflict { expected, actual });
        }
        // Compare file identity without a second read handle: Windows locks
        // are mandatory, so reading our own locked file through it would fail.
        if identity != same_file::Handle::from_path(path)? {
            return Err(SourceError::Unsupported(
                "file was replaced while saving".into(),
            ));
        }
        std::fs::rename(&tmp, path)?;
        Ok(bytes_version(next.as_bytes()))
    })();
    if result.is_err() && created {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod race_tests {
    use super::*;

    #[tokio::test]
    async fn listing_large_files_only_reads_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(64 << 20)
            .unwrap();
        let src = FolderSource::open(dir.path()).unwrap();
        // Match FolderSource's canonical root (Windows verbatim paths, macOS /private).
        let path = std::fs::canonicalize(path).unwrap();
        CONTENT_OPENS.lock().unwrap().insert(path.clone(), 0);
        let id = src.node_id("large.bin");
        let hint = FolderSource::version_of(&std::fs::metadata(&path).unwrap());
        for query in [
            Query::Children(src.root_id()),
            Query::Text {
                dialect: "ls".into(),
                text: "".into(),
            },
            Query::Text {
                dialect: "path".into(),
                text: "large.bin".into(),
            },
            Query::Node(id),
            Query::Neighbours {
                node: src.root_id(),
                depth: 1,
                direction: moonkale_core::Direction::Out,
            },
        ] {
            let result = src.query(query).await.unwrap();
            assert_eq!(
                *CONTENT_OPENS.lock().unwrap().get(&path).unwrap(),
                0,
                "listing opened file content"
            );
            assert_eq!(
                result.nodes.iter().find(|n| n.id == id).unwrap().version,
                hint
            );
        }
        let (bytes, version) = src.fetch_bytes(id).await.unwrap();
        assert_eq!(
            *CONTENT_OPENS.lock().unwrap().get(&path).unwrap(),
            1,
            "counter covers real content reads"
        );
        assert_eq!(version, bytes_version(&bytes));
        assert_ne!(version, hint);
        CONTENT_OPENS.lock().unwrap().remove(&path);
    }

    #[test]
    fn changes_during_staging_are_refused_and_temps_removed() {
        for replace_path in [false, true] {
            // Windows mandatory locks prevent the in-place external write.
            if cfg!(windows) && !replace_path {
                continue;
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.md");
            std::fs::write(&path, "old").unwrap();
            let expected = bytes_version(b"old");
            let result = save_text_checked(
                &path,
                expected,
                &moonkale_core::TextPatch::whole("bad", 3),
                || {
                    if replace_path {
                        let other = dir.path().join("replacement");
                        std::fs::write(&other, "new").unwrap();
                        std::fs::rename(other, &path).unwrap();
                    } else {
                        std::fs::write(&path, "new").unwrap();
                    }
                },
            );
            assert!(matches!(
                result,
                Err(SourceError::Conflict { .. } | SourceError::Unsupported(_))
            ));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn native_rename_exclusion_preserves_an_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a");
        let dst = dir.path().join("b");
        std::fs::write(&src, "a").unwrap();
        std::fs::write(&dst, "b").unwrap();
        assert_eq!(
            rename_no_replace(&src, &dst).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&src).unwrap(), b"a");
        assert_eq!(std::fs::read(&dst).unwrap(), b"b");
        std::fs::remove_file(&dst).unwrap();
        rename_no_replace(&src, &dst).unwrap();
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"a");
    }
}
