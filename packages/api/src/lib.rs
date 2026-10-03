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

#[cfg(feature = "server")]
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
pub mod host_state;
#[cfg(not(target_arch = "wasm32"))]
pub mod relay;
mod remote;
mod terminal;
pub use llm::ProviderInfo;
#[cfg(target_arch = "wasm32")]
pub use llm::RemoteProvider;
pub use lsp::RemoteLsp;
pub use remote::RemoteSource;
pub use terminal::RemoteTerminal;

#[cfg(feature = "server")]
pub(crate) mod state {
    use moonkale_project_fs::FolderSource;
    use moonkale_sources::SourceRegistry;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, OnceLock};

    static REGISTRY: OnceLock<SourceRegistry> = OnceLock::new();

    pub fn registry() -> &'static SourceRegistry {
        REGISTRY.get_or_init(SourceRegistry::new)
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
                ]
                .concat(),
            )
        })
    }

    pub use moonkale_server_host::{allowed_root, jail_dir};

    /// A path one of [`openers`] claims opens as that database; anything
    /// else as a folder.
    pub async fn open_any(path: &str) -> std::io::Result<Vec<Arc<dyn moonkale_core::Source>>> {
        let allowed = std::fs::canonicalize(allowed_root())?;
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
        // Databases: the first of the server's source openers that claims
        // the path (Milestone 18 phase 2; the same list the desktop uses).
        if let Some(db) = openers()
            .open(canonical.clone(), canonical.is_dir())
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?
        {
            return Ok(vec![db]);
        }
        Ok(vec![Arc::new(FolderSource::open(canonical)?)])
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
    let reg = state::registry();
    let mut out = Vec::new();
    for source in state::open_any(&path).await.map_err(server_error)? {
        let is_folder = source.descriptor().family == moonkale_core::SourceFamily::Folder;
        out.push(reg.insert(source.clone()));
        if is_folder {
            // Embeddings (if the client's settings ask for them) are filled in
            // the background so the folder opens at once; search is BM25-only
            // until they arrive. The secret is resolved on the server.
            let embedder = embed.as_ref().and_then(llm::provider_for);
            let index = std::sync::Arc::new(
                moonkale_index::IndexSource::build_with(source, embedder)
                    .await
                    .map_err(server_error)?,
            );
            out.push(reg.insert(index.clone()));
            tokio::spawn(async move {
                match index.embed_pending().await {
                    Ok(n) if n > 0 => eprintln!("moonkale: embedded {n} chunks"),
                    Ok(_) => {}
                    Err(e) => eprintln!("moonkale: embedding failed: {e}"),
                }
            });
        }
    }
    Ok(out)
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

/// Compile a Typst document on the server. `root` must be inside
/// `MOONKALE_ROOT`; `main_rel` is `/`-rooted relative to it.
#[post("/api/typst/compile")]
pub async fn compile_typst(
    root: String,
    main_rel: String,
    text: String,
) -> Result<Result<Vec<String>, Vec<String>>, ServerFnError> {
    let root = state::jail_dir(Some(&root)).map_err(server_error)?;
    let out = tokio::task::spawn_blocking(move || {
        moonkale_typst::compile_to_svg(std::path::Path::new(&root), &main_rel, text)
    })
    .await
    .map_err(server_error)?;
    Ok(out.map_err(|d| {
        d.into_iter()
            .map(|d| match d.hint {
                Some(h) => format!("{} (hint: {h})", d.message),
                None => d.message,
            })
            .collect()
    }))
}
