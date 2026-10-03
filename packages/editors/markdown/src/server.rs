//! Markdown's server half (Milestone 18 phase 4.3; was `api::compile_typst`):
//! the Typst preview compiles on the server for the web client and for a
//! desktop connected to one. Feature `server` brings the compiler; without
//! it this is the client stub plus [`remote_typst`].

use dioxus::prelude::*;

/// Compile a Typst document on the server. `root` must be inside
/// `MOONKALE_ROOT`; `main_rel` is `/`-rooted relative to it.
#[post("/api/typst/compile")]
pub async fn compile_typst(
    root: String,
    main_rel: String,
    text: String,
) -> Result<Result<Vec<String>, Vec<String>>, ServerFnError> {
    let root = moonkale_server_host::jail_dir(Some(&root)).map_err(ServerFnError::new)?;
    let out = tokio::task::spawn_blocking(move || {
        moonkale_typst::compile_to_svg(std::path::Path::new(&root), &main_rel, text)
    })
    .await
    .map_err(ServerFnError::new)?;
    Ok(out.map_err(|d| {
        d.into_iter()
            .map(|d| match d.hint {
                Some(h) => format!("{} (hint: {h})", d.message),
                None => d.message,
            })
            .collect()
    }))
}

/// A `CompileTypst` over [`compile_typst`]: the platform's Typst where the
/// folder is on a server.
pub fn remote_typst(
    root: String,
    main_rel: String,
    text: String,
) -> moonkale_ext_api::CompileTypstFuture {
    Box::pin(async move {
        compile_typst(root, main_rel, text)
            .await
            .unwrap_or_else(|e| Err(vec![e.to_string()]))
    })
}
