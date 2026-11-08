//! # api — the server, and the client's view of it
//!
//! Dioxus fullstack server functions. Each `#[post]` function below is, on
//! the server, an HTTP endpoint backed by real sources; on the client, a
//! stub that makes the request. [`RemoteSource`] wraps those stubs in the
//! `Source` trait so the shell cannot tell a server-side folder from a local
//! one.
//!
//! ```text
//!   web client                         server (feature = "server")
//!   RemoteSource ──open_folder()──►    REGISTRY: SourceRegistry
//!                ──query()──────►      └─ FolderSource(s)
//!                ──fetch_text()─►
//!                ──apply()──────►
//! ```
//!
//! Security: `open_folder` accepts a *server* path, confined to
//! `MOONKALE_ROOT`; with `MOONKALE_TOKEN` set every request needs the token
//! ([`auth`]). The trust model and the open findings of the 2026-09-23 audit
//! are in the vault: `architecture/Security.md`.

use dioxus::prelude::*;
use moonkale_core::{
    Applied, NodeId, Query, QueryResult, SourceDescriptor, SourceError, SourceId, Transaction,
    Version,
};

#[cfg(any(feature = "server", all(test, not(target_arch = "wasm32"))))]
pub mod auth;
mod llm;
mod lsp;
#[cfg(feature = "server")]
pub mod mcp;
pub mod presence;
mod wasm;
#[cfg(feature = "server")]
pub use wasm::module_bytes;
pub use wasm::{list_wasm_extensions, run_wasm_command};
pub mod agent_sessions;
pub mod client;
pub mod handshake;
pub mod host_state;
#[cfg(not(target_arch = "wasm32"))]
pub mod relay;
mod remote;
#[cfg(feature = "structural-edits")]
pub mod structural;
mod terminal;
pub mod themes;
pub use llm::ProviderInfo;
#[cfg(target_arch = "wasm32")]
pub use llm::RemoteProvider;
pub use lsp::RemoteLsp;
pub use remote::RemoteSource;
pub use terminal::RemoteTerminal;

#[cfg(any(feature = "server", all(test, not(target_arch = "wasm32"))))]
pub(crate) mod reopen {
    use moonkale_core::{Source, SourceDescriptor, SourceFamily};
    use std::collections::HashMap;
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct OpenedPath {
        sources: Vec<Arc<dyn Source>>,
        embed: Option<moonkale_llm::LlmSettings>,
    }

    #[derive(Default)]
    pub struct OpenTable {
        by_path: Mutex<HashMap<PathBuf, OpenedPath>>,
    }

    pub fn canonical_in_root(allowed_root: &Path, path: &str) -> std::io::Result<PathBuf> {
        let allowed = std::fs::canonicalize(allowed_root)?;
        let requested: PathBuf = if path.trim().is_empty() {
            allowed.clone()
        } else if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            allowed.join(path)
        };
        let canonical = std::fs::canonicalize(&requested)?;
        if !canonical.starts_with(&allowed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} is outside MOONKALE_ROOT ({})",
                    canonical.display(),
                    allowed.display()
                ),
            ));
        }
        Ok(canonical)
    }

    impl OpenTable {
        pub fn new() -> Self {
            Self::default()
        }

        /// Return descriptors for `path` (canonicalised under `allowed_root`),
        /// reusing the open sources when `canonical` is already open. If a
        /// folder is re-opened with new `Some(LlmSettings)` embedding options,
        /// `build` is called with the existing `FolderSource` so only the index
        /// is rebuilt.
        pub async fn open_or_reuse<F, Fut>(
            &self,
            allowed_root: &Path,
            path: &str,
            embed: Option<moonkale_llm::LlmSettings>,
            build: F,
        ) -> Result<Vec<SourceDescriptor>, String>
        where
            F: FnOnce(PathBuf, Option<Arc<dyn Source>>, Option<moonkale_llm::LlmSettings>) -> Fut,
            Fut: Future<Output = Result<Vec<Arc<dyn Source>>, String>>,
        {
            let canonical = canonical_in_root(allowed_root, path).map_err(|e| e.to_string())?;
            let mut guard = self.by_path.lock().await;
            let existing_folder = if let Some(entry) = guard.get(&canonical) {
                let is_folder = entry
                    .sources
                    .first()
                    .is_some_and(|s| s.descriptor().family == SourceFamily::Folder);
                // Only folders are reused (their index is the costly part). A
                // database file opens fresh each time, as before: a source
                // that once fell back to read-only (a locked file) must not
                // refuse every later write (CI on 6c5c1e3e, table-edit).
                if !is_folder {
                    None
                } else if embed.is_none() || embed == entry.embed {
                    return Ok(entry.sources.iter().map(|s| s.descriptor()).collect());
                } else {
                    entry.sources.first().cloned()
                }
            } else {
                None
            };
            let sources = build(canonical.clone(), existing_folder, embed.clone()).await?;
            let descriptors = sources.iter().map(|s| s.descriptor()).collect();
            guard.insert(canonical, OpenedPath { sources, embed });
            Ok(descriptors)
        }
    }
}

#[cfg(feature = "server")]
pub(crate) mod state {
    use moonkale_project_fs::FolderSource;
    use moonkale_sources::SourceRegistry;
    use std::path::Path;
    use std::sync::{Arc, OnceLock};

    static REGISTRY: OnceLock<SourceRegistry> = OnceLock::new();
    static OPENED: OnceLock<crate::reopen::OpenTable> = OnceLock::new();

    pub fn registry() -> &'static SourceRegistry {
        REGISTRY.get_or_init(SourceRegistry::new)
    }

    pub fn opened() -> &'static crate::reopen::OpenTable {
        OPENED.get_or_init(crate::reopen::OpenTable::new)
    }

    /// The server's source openers (every driver this server is built with).
    pub fn openers() -> &'static moonkale_core::Openers {
        static OPENERS: OnceLock<moonkale_core::Openers> = OnceLock::new();
        OPENERS.get_or_init(|| {
            moonkale_core::Openers::new(
                [
                    moonkale_sources_sql::openers(),
                    moonkale_sources_kv::openers(),
                    moonkale_sources_graph::openers(),
                    #[cfg(feature = "unison")]
                    moonkale_sources_unison::openers(),
                    #[cfg(feature = "sophia")]
                    moonkale_sources_sophia::openers(),
                ]
                .concat(),
            )
        })
    }

    pub use moonkale_server_host::{allowed_root, jail_dir};

    /// Open a canonical path as a database (if one of [`openers`] claims it)
    /// or as a folder.
    pub async fn open_canonical(
        canonical: &Path,
    ) -> std::io::Result<Vec<Arc<dyn moonkale_core::Source>>> {
        // Databases: the first of the server's source openers that claims
        // the path (Milestone 18 phase 2; the same list the desktop uses).
        if let Some(db) = openers()
            .open(canonical.to_path_buf(), canonical.is_dir())
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?
        {
            return Ok(vec![db]);
        }
        let folder: Arc<dyn moonkale_core::Source> = Arc::new(FolderSource::open(canonical)?);
        #[cfg(feature = "sophia")]
        let folder = if folder.descriptor().truth == moonkale_core::SourceTruth::Store {
            match moonkale_index::folder_store::store_folder::StoreFolder::open(folder.clone()) {
                Ok(source) => Arc::new(source) as Arc<dyn moonkale_core::Source>,
                Err(_) => folder,
            }
        } else {
            folder
        };
        Ok(vec![folder])
    }

    pub async fn open_folder(
        path: &str,
        embed: Option<moonkale_llm::LlmSettings>,
    ) -> Result<Vec<moonkale_core::SourceDescriptor>, String> {
        let root = allowed_root();
        opened()
            .open_or_reuse(
                &root,
                path,
                embed,
                |canonical, existing_folder, embed| async move {
                    let reg = registry();
                    let initial = match existing_folder {
                        Some(folder) => vec![folder],
                        None => open_canonical(&canonical)
                            .await
                            .map_err(|e| e.to_string())?,
                    };
                    let mut out: Vec<Arc<dyn moonkale_core::Source>> = Vec::new();
                    for source in initial {
                        let is_folder =
                            source.descriptor().family == moonkale_core::SourceFamily::Folder;
                        if is_folder {
                            // Embeddings (if the client's settings ask for them) are filled in
                            // the background so the folder opens at once; search is BM25-only
                            // until they arrive. The secret is resolved on the server.
                            let embedder = embed.as_ref().and_then(crate::llm::provider_for);
                            let index = Arc::new(
                                moonkale_index::IndexSource::build_with(source.clone(), embedder)
                                    .await
                                    .map_err(|e| e.to_string())?,
                            );
                            reg.insert(source.clone());
                            reg.insert(index.clone());
                            out.push(source);
                            out.push(index.clone());
                            #[cfg(feature = "sophia")]
                            if let Some(path) = index.folder_store_path() {
                                let sophia_id = moonkale_core::SourceId::new(format!(
                                    "sophia:{}",
                                    path.display()
                                ));
                                let sophia_res = match reg.get(&sophia_id) {
                                    Some(s) => Ok(s),
                                    None => moonkale_sources_sophia::SophiaSource::open(path)
                                        .await
                                        .map(|s| Arc::new(s) as Arc<dyn moonkale_core::Source>),
                                };
                                match sophia_res {
                                    Ok(source) => {
                                        index.set_cache_reader(Ok(source.clone()));
                                        reg.insert(source.clone());
                                        out.push(source);
                                    }
                                    Err(error) => index.set_cache_reader(Err(error)),
                                }
                            }
                            tokio::spawn(async move {
                                match index.embed_pending().await {
                                    Ok(n) if n > 0 => eprintln!("moonkale: embedded {n} chunks"),
                                    Ok(_) => {}
                                    Err(e) => eprintln!("moonkale: embedding failed: {e}"),
                                }
                            });
                        } else {
                            reg.insert(source.clone());
                            out.push(source);
                        }
                    }
                    Ok(out)
                },
            )
            .await
    }
}

#[cfg(feature = "server")]
fn server_error(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

/// Echo the user input on the server. Kept from the template; doubles as the
/// smoke test that server functions work at all.
#[post("/api/echo")]
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    Ok(input)
}

/// Open a folder on the server, index it, and register both as sources.
/// Returns the folder's descriptor first, then the index's.
#[post("/api/sources/open_folder")]
pub async fn open_folder(
    path: String,
    embed: Option<moonkale_llm::LlmSettings>,
) -> Result<Vec<SourceDescriptor>, ServerFnError> {
    state::open_folder(&path, embed).await.map_err(server_error)
}

/// A node was written; let a derived source re-read it.
#[post("/api/sources/refresh")]
pub async fn refresh_source(
    source: SourceId,
    node: NodeId,
) -> Result<Result<(), SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.refresh(node).await)
}

/// Preserve one upstream debounce window across the remote Source boundary.
#[post("/api/sources/refresh_batch")]
pub async fn refresh_source_batch(
    source: SourceId,
    nodes: Vec<NodeId>,
) -> Result<Result<(), SourceError>, ServerFnError> {
    if nodes.len() > 10_000 {
        return Ok(Err(SourceError::Invalid("refresh batch limit".into())));
    }
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.refresh_batch(&nodes).await)
}

/// Changes made on the server's disk after `since` (Milestone 16) — a long
/// poll, see `Source::changes_since`. `Ok(None)`: that source is not watched.
/// Every client (browser, phone, a desktop on a remote folder) follows the
/// folder through this one request per source; it holds for at most ~25 s.
#[post("/api/sources/changes")]
pub async fn source_changes(
    source: SourceId,
    since: u64,
) -> Result<Result<Option<moonkale_core::Changes>, SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.changes_since(since).await)
}

/// Descriptors of every open source.
#[post("/api/sources/list")]
pub async fn list_sources() -> Result<Vec<SourceDescriptor>, ServerFnError> {
    Ok(state::registry().descriptors())
}

#[post("/api/sources/query")]
pub async fn query_source(
    source: SourceId,
    q: Query,
) -> Result<Result<QueryResult, SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.query(q).await)
}

#[post("/api/sources/fetch_text")]
pub async fn fetch_text_from(
    source: SourceId,
    node: NodeId,
) -> Result<Result<(String, Version), SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.fetch_text(node).await)
}

/// Bytes come back base64-encoded (JSON transport); the client decodes.
#[post("/api/sources/fetch_bytes")]
pub async fn fetch_bytes_from(
    source: SourceId,
    node: NodeId,
) -> Result<Result<(String, Version), SourceError>, ServerFnError> {
    use base64::Engine;
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.fetch_bytes(node)
        .await
        .map(|(bytes, v)| (base64::engine::general_purpose::STANDARD.encode(bytes), v)))
}

#[post("/api/sources/apply")]
pub async fn apply_to(
    source: SourceId,
    tx: Transaction,
) -> Result<Result<Applied, SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.apply(tx).await)
}

/// Preview on the source host; the ordinary authenticated API guard applies.
#[post("/api/sources/preview_text")]
pub async fn preview_text_at(
    source: SourceId,
    tx: Transaction,
) -> Result<Result<(), SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.preview_text(tx).await)
}

/// Read authoritative row identities on the authenticated source host.
#[post("/api/sources/read_rows")]
pub async fn read_rows_at(
    source: SourceId,
    table: String,
    limit: usize,
) -> Result<Result<moonkale_core::RowPage, SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.read_rows(table, limit).await)
}

/// Typed prepared table query; the source validates schema and bounds.
#[post("/api/sources/query_rows")]
pub async fn query_rows_at(
    source: SourceId,
    table: String,
    query: moonkale_core::TableQuery,
) -> Result<Result<moonkale_core::RowPage, SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.query_rows(table, query).await)
}

#[post("/api/sources/preview_rows")]
pub async fn preview_rows_at(
    source: SourceId,
    batch: moonkale_core::RowBatch,
) -> Result<Result<(), SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.preview_rows(batch).await)
}

#[post("/api/sources/apply_rows")]
pub async fn apply_rows_at(
    source: SourceId,
    batch: moonkale_core::RowBatch,
) -> Result<Result<Vec<Option<moonkale_core::RowSnapshot>>, SourceError>, ServerFnError> {
    let s = state::registry()
        .get(&source)
        .ok_or_else(|| server_error("unknown source"))?;
    Ok(s.apply_rows(batch).await)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod reopen_tests {
    use super::reopen::OpenTable;
    use moonkale_core::{
        async_trait, Applied, Capabilities, NodeId, Query, QueryResult, Source, SourceDescriptor,
        SourceError, SourceFamily, SourceId, SourceTruth, Transaction, Version,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct FakeSource {
        descriptor: SourceDescriptor,
    }

    impl FakeSource {
        fn arc(id: &str, family: SourceFamily, display_name: &str) -> Arc<dyn Source> {
            let sid = SourceId::new(id);
            Arc::new(Self {
                descriptor: SourceDescriptor {
                    truth: SourceTruth::Files,
                    root: NodeId::derive(&sid, ""),
                    id: sid,
                    display_name: display_name.into(),
                    family,
                    capabilities: Capabilities::default(),
                },
            })
        }
    }

    #[async_trait]
    impl Source for FakeSource {
        fn id(&self) -> SourceId {
            self.descriptor.id.clone()
        }
        fn descriptor(&self) -> SourceDescriptor {
            self.descriptor.clone()
        }
        async fn query(&self, _query: Query) -> Result<QueryResult, SourceError> {
            Ok(QueryResult::default())
        }
        async fn fetch_text(&self, _node: NodeId) -> Result<(String, Version), SourceError> {
            Err(SourceError::NotFound)
        }
        async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
            Err(SourceError::Unsupported("test".into()))
        }
    }

    #[test]
    fn reopening_same_canonical_folder_builds_once_and_reuses_descriptors() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let sub = dir.path().join("sub");
            std::fs::create_dir_all(&sub).unwrap();
            let table = OpenTable::new();
            let folder_builds = AtomicUsize::new(0);
            let index_builds = AtomicUsize::new(0);

            let build = |canonical: std::path::PathBuf,
                         existing_folder: Option<Arc<dyn Source>>,
                         embed: Option<moonkale_llm::LlmSettings>| {
                let folder_builds = &folder_builds;
                let index_builds = &index_builds;
                async move {
                    let folder = match existing_folder {
                        Some(f) => f,
                        None => {
                            let n = folder_builds.fetch_add(1, Ordering::SeqCst) + 1;
                            FakeSource::arc(
                                &format!("folder:{}", canonical.display()),
                                SourceFamily::Folder,
                                &format!("folder#{n}"),
                            )
                        }
                    };
                    let n = index_builds.fetch_add(1, Ordering::SeqCst) + 1;
                    let tag = embed
                        .as_ref()
                        .map(|e| e.provider.as_str())
                        .unwrap_or("none");
                    let index = FakeSource::arc(
                        &format!("index:{}", folder.id()),
                        SourceFamily::Index,
                        &format!("index#{n}:{tag}"),
                    );
                    Ok(vec![folder, index])
                }
            };

            let first = table
                .open_or_reuse(dir.path(), "", None, &build)
                .await
                .unwrap();
            let second = table
                .open_or_reuse(dir.path(), "sub/..", None, &build)
                .await
                .unwrap();
            assert_eq!(first, second);
            assert_eq!(folder_builds.load(Ordering::SeqCst), 1);
            assert_eq!(index_builds.load(Ordering::SeqCst), 1);

            // Re-opening with new embedding settings reuses the FolderSource and
            // rebuilds only the index; a later open with the same settings or
            // `None` reuses that index without rebuilding.
            let embed = Some(moonkale_llm::LlmSettings {
                provider: "mock".into(),
                ..Default::default()
            });
            let third = table
                .open_or_reuse(dir.path(), ".", embed.clone(), &build)
                .await
                .unwrap();
            assert_eq!(folder_builds.load(Ordering::SeqCst), 1);
            assert_eq!(index_builds.load(Ordering::SeqCst), 2);
            assert_eq!(third[0], first[0]);
            assert_eq!(third[1].display_name, "index#2:mock");

            let fourth = table
                .open_or_reuse(dir.path(), "", embed, &build)
                .await
                .unwrap();
            let fifth = table
                .open_or_reuse(dir.path(), "", None, &build)
                .await
                .unwrap();
            assert_eq!(fourth, third);
            assert_eq!(fifth, third);
            assert_eq!(folder_builds.load(Ordering::SeqCst), 1);
            assert_eq!(index_builds.load(Ordering::SeqCst), 2);
        });
    }
}

#[cfg(all(test, feature = "server"))]
mod server_open_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn open_folder_reuses_registered_sources_for_same_folder() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("Note.md"), "# Note\n\nSee [[Other]].\n").unwrap();
            std::env::set_var("MOONKALE_ROOT", dir.path());

            let first = state::open_folder("", None).await.unwrap();
            assert!(first.len() >= 2);
            let folder_a = state::registry().get(&first[0].id).unwrap();
            let index_a = state::registry().get(&first[1].id).unwrap();

            let second = state::open_folder(".", None).await.unwrap();
            assert_eq!(first, second);
            let folder_b = state::registry().get(&second[0].id).unwrap();
            let index_b = state::registry().get(&second[1].id).unwrap();
            assert!(Arc::ptr_eq(&folder_a, &folder_b));
            assert!(Arc::ptr_eq(&index_a, &index_b));
        });
    }
}
