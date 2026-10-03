//! Git's server half (Milestone 18 phase 4.3; was `moonkale_server::git_run`): the web
//! client — and a desktop connected to a server — sends a request for the
//! folder it has open; the server runs the `git` CLI inside the jail.
//!
//! This is also the spike of phase 4.3: a server function defined in an
//! extension crate is registered by Dioxus when the crate is linked into the
//! server binary (with this crate's `server` feature on), with no line in
//! `api`.

use crate::types::{GitRequest, GitResponse};
use dioxus::prelude::*;

#[post("/api/git")]
pub async fn git_run(
    root: String,
    req: GitRequest,
) -> Result<Result<GitResponse, String>, ServerFnError> {
    let dir = match moonkale_server_host::jail_dir(Some(&root)) {
        Ok(d) => d,
        Err(e) => return Ok(Err(e.to_string())),
    };
    Ok(crate::cli::run(std::path::Path::new(&dir), req).await)
}

/// A [`crate::GitRunner`] over [`git_run`]: git where the folder is on a
/// server (the web client; a desktop or phone connected to one).
pub fn remote(root: String, req: GitRequest) -> moonkale_ext_api::SettingsFuture<GitResponse> {
    Box::pin(async move {
        match git_run(root, req).await {
            Ok(r) => r,
            Err(e) => Err(e.to_string()),
        }
    })
}
