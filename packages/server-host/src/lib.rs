//! # moonkale-server-host — what every server half shares
//!
//! Milestone 18 phase 4.3: an extension's server half (its `server`
//! feature: server functions, routes) needs the server's rules without
//! depending on `api`, which assembles everything. Today that is **the
//! jail**: every path a client names is resolved inside `MOONKALE_ROOT`.
//! Auth and the route registry stay in `api` (the host binary's side), and
//! a server half never sees a request that did not pass them.

use std::path::{Path, PathBuf};

/// Where `open_folder` may look. `MOONKALE_ROOT` if set, else the
/// process working directory (which under `dx serve` is the workspace).
pub fn allowed_root() -> PathBuf {
    std::env::var_os("MOONKALE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
}

/// A directory inside the allowed root (blank = the root), canonical.
pub fn jail_dir(path: Option<&str>) -> std::io::Result<String> {
    let allowed = std::fs::canonicalize(allowed_root())?;
    let requested: PathBuf = match path.map(str::trim).filter(|p| !p.is_empty()) {
        None => allowed.clone(),
        Some(p) if Path::new(p).is_absolute() => PathBuf::from(p),
        Some(p) => allowed.join(p),
    };
    let canonical = std::fs::canonicalize(&requested)?;
    if !canonical.starts_with(&allowed) || !canonical.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "{} is not a directory inside MOONKALE_ROOT",
                canonical.display()
            ),
        ));
    }
    Ok(canonical.to_string_lossy().into_owned())
}
