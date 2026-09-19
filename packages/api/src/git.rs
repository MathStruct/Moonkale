//! Git on the server (Milestone 7): the web client sends a request for the
//! folder it has open; the server runs the `git` CLI inside the jail. Same
//! dev-server trust model as the terminal and LSP relays (P-20 / step 6).

use dioxus::prelude::*;
use moonkale_ext_api::git::{GitRequest, GitResponse};

#[post("/api/git")]
pub async fn git_run(
    root: String,
    req: GitRequest,
) -> Result<Result<GitResponse, String>, ServerFnError> {
    let dir = match crate::state::jail_dir(Some(&root)) {
        Ok(d) => d,
        Err(e) => return Ok(Err(e.to_string())),
    };
    Ok(moonkale_ext_git::cli::run(std::path::Path::new(&dir), req).await)
}
