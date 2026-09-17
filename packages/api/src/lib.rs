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
//! Security (Milestone 1): `open_folder` accepts a *server* path. Set
//! `MOONKALE_ROOT` to confine it to one subtree; there is no authentication
//! yet. Treat `dx serve` as a local dev tool until the security items in the
//! vault's Platform Matrix are done.

use dioxus::prelude::*;
use moonkale_core::{
    Applied, NodeId, Query, QueryResult, SourceDescriptor, SourceError, SourceId, Transaction,
    Version,
};

mod remote;
pub use remote::RemoteSource;

#[cfg(feature = "server")]
mod state {
    use moonkale_project_fs::FolderSource;
    use moonkale_sources::SourceRegistry;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, OnceLock};

    static REGISTRY: OnceLock<SourceRegistry> = OnceLock::new();

    pub fn registry() -> &'static SourceRegistry {
        REGISTRY.get_or_init(SourceRegistry::new)
    }

    /// Where `open_folder` may look. `MOONKALE_ROOT` if set, else the
    /// process working directory (which under `dx serve` is the workspace).
    pub fn allowed_root() -> PathBuf {
        std::env::var_os("MOONKALE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
    }

    pub fn open(path: &str) -> std::io::Result<Arc<FolderSource>> {
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
        Ok(Arc::new(FolderSource::open(canonical)?))
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

/// Open a folder on the server and register it as a source.
#[post("/api/sources/open_folder")]
pub async fn open_folder(path: String) -> Result<SourceDescriptor, ServerFnError> {
    let source = state::open(&path).map_err(server_error)?;
    Ok(state::registry().insert(source))
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
