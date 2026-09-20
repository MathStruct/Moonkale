//! The `Source` impl over a local folder.
//!
//! - Ids: `NodeId::derive(source_id, relative_path)`, the root is `""`.
//! - Versions: a hash of `(mtime, len)`. Cheap, changes on any external
//!   write, and good enough for optimistic concurrency in Milestone 1. Known
//!   gap: two writes inside one mtime tick with equal length collide; a
//!   content hash will replace it once the index keeps hashes anyway.
//! - Writes: apply the patch to the current text, write to a sibling temp
//!   file, `rename` over the original. Readers never see a half-written file.
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
}

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
        Version(h.finish())
    }

    async fn stat(&self, rel: &str) -> Result<(std::fs::Metadata, Version), SourceError> {
        let meta = tokio::fs::metadata(tree::absolute(&self.root, rel)).await?;
        let v = Self::version_of(&meta);
        Ok((meta, v))
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
                watch: false,
                text_query: None,
            },
            root: self.root_id(),
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Neighbours { node, .. } => {
                // A folder's neighbourhood is containment: its children.
                self.query(Query::Children(node)).await
            }
            // `path`: resolve a relative path directly (hidden files too, e.g.
            // `.moonkale/settings.json`), without walking the tree.
            Query::Text { dialect, text } if dialect == "path" => {
                let rel = text.trim().trim_start_matches("./").trim_matches('/').to_string();
                if rel.split('/').any(|p| p == "..") {
                    return Err(SourceError::Invalid("path escapes the folder".into()));
                }
                let (meta, v) = self.stat(&rel).await?;
                Ok(QueryResult::single(self.node_for(
                    &rel,
                    meta.is_dir(),
                    meta.len(),
                    v,
                )))
            }
            Query::All { .. } | Query::Text { .. } => Err(SourceError::Unsupported(
                "folders answer Node/Children/Neighbours and Text{path}; the index has the whole graph"
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
                let dir = tree::absolute(&self.root, &rel);
                let root = self.root.clone();
                let entries = tokio::task::spawn_blocking(move || tree::list_children(&root, &dir))
                    .await
                    .map_err(|e| SourceError::Io(e.to_string()))??;
                let mut result = QueryResult::default();
                for e in entries {
                    let version = if e.is_dir {
                        Version::default()
                    } else {
                        self.stat(&e.rel).await.map(|(_, v)| v).unwrap_or_default()
                    };
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
        let path = tree::absolute(&self.root, &rel);
        let (_, version) = self.stat(&rel).await?;
        let text = tokio::fs::read_to_string(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                SourceError::Unsupported("not valid UTF-8".into())
            } else {
                e.into()
            }
        })?;
        Ok((text, version))
    }

    async fn fetch_bytes(&self, node: NodeId) -> Result<(Vec<u8>, Version), SourceError> {
        let rel = self.rel_of(node)?;
        let path = tree::absolute(&self.root, &rel);
        let (_, version) = self.stat(&rel).await?;
        let bytes = tokio::fs::read(&path).await?;
        Ok((bytes, version))
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
}

impl FolderSource {
    /// Create `name` under the directory `parent`; refuses to overwrite.
    async fn create_text(
        &self,
        parent: NodeId,
        name: &str,
        text: &str,
    ) -> Result<(NodeId, Version), SourceError> {
        let parent_rel = self.rel_of(parent)?;
        let name = name.trim_matches('/');
        if name.is_empty() || name.split('/').any(|p| p == "..") {
            return Err(SourceError::Invalid(format!("bad name {name:?}")));
        }
        let rel = if parent_rel.is_empty() {
            name.to_string()
        } else {
            format!("{parent_rel}/{name}")
        };
        let path = tree::absolute(&self.root, &rel);
        if tokio::fs::metadata(&path).await.is_ok() {
            return Err(SourceError::Invalid(format!("{rel} already exists")));
        }
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::write(&path, text.as_bytes()).await?;
        let (_, version) = self.stat(&rel).await?;
        Ok((self.node_id(&rel), version))
    }

    /// `parent/name` as a checked relative path (no `..`, not empty).
    fn child_rel(&self, parent: NodeId, name: &str) -> Result<String, SourceError> {
        let parent_rel = self.rel_of(parent)?;
        let name = name.trim_matches('/');
        if name.is_empty() || name.split('/').any(|p| p == "..") {
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
        let path = tree::absolute(&self.root, &rel);
        if tokio::fs::metadata(&path).await.is_ok() {
            return Err(SourceError::Invalid(format!("{rel} already exists")));
        }
        tokio::fs::create_dir_all(&path).await?;
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
        if to.is_empty() || to.split('/').any(|p| p == "..") {
            return Err(SourceError::Invalid(format!("bad path {to:?}")));
        }
        if to == from {
            let (_, version) = self.stat(&from).await?;
            return Ok((node, version));
        }
        let src = tree::absolute(&self.root, &from);
        let dst = tree::absolute(&self.root, to);
        if tokio::fs::metadata(&dst).await.is_ok() {
            return Err(SourceError::Invalid(format!("{to} already exists")));
        }
        if to.starts_with(&format!("{from}/")) {
            return Err(SourceError::Invalid(format!(
                "cannot move {from} into itself"
            )));
        }
        if let Some(dir) = dst.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::rename(&src, &dst).await?;
        self.known.write().unwrap().remove(&node);
        let (_, version) = self.stat(to).await?;
        Ok((self.node_id(to), version))
    }

    /// Delete = move into `.moonkale/trash/<unix-ms>/<path>` so a slip can be
    /// undone by hand; the trash is git-ignored like the rest of `.moonkale`.
    async fn delete(&self, node: NodeId) -> Result<(), SourceError> {
        let rel = self.rel_of(node)?;
        if rel.is_empty() {
            return Err(SourceError::Invalid("cannot delete the root".into()));
        }
        let src = tree::absolute(&self.root, &rel);
        tokio::fs::metadata(&src).await?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let dst = self
            .root
            .join(".moonkale")
            .join("trash")
            .join(stamp.to_string())
            .join(&rel);
        if let Some(dir) = dst.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::rename(&src, &dst).await?;
        self.known.write().unwrap().remove(&node);
        Ok(())
    }

    async fn write_text(
        &self,
        node: NodeId,
        expected: Version,
        patch: &moonkale_core::TextPatch,
    ) -> Result<Version, SourceError> {
        let rel = self.rel_of(node)?;
        let path = tree::absolute(&self.root, &rel);
        let (meta, actual) = self.stat(&rel).await?;
        if meta.is_dir() {
            return Err(SourceError::Unsupported(
                "cannot write text to a directory".into(),
            ));
        }
        if actual != expected {
            return Err(SourceError::Conflict { expected, actual });
        }
        let current = tokio::fs::read_to_string(&path).await?;
        let next = patch.apply(&current)?;
        // Atomic replace: write a sibling temp file, then rename over.
        let tmp = path.with_extension(format!(
            "{}.moonkale-tmp",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));
        tokio::fs::write(&tmp, next.as_bytes()).await?;
        tokio::fs::rename(&tmp, &path).await?;
        let (_, version) = self.stat(&rel).await?;
        Ok(version)
    }
}
