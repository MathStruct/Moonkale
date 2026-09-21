//! The desktop as a client of a Moonkale server (Milestone 11, step 1):
//! `api::client::connect` points the server functions at `MOONKALE_REMOTE`,
//! after which the desktop's config callbacks open folders, read files, run
//! git and wasm commands *there*. Ignored unless a server is running, e.g.
//! `MOONKALE_REMOTE=http://127.0.0.1:8090 cargo test -p desktop --test remote -- --ignored`
//! (with `MOONKALE_TOKEN` when the server has one).

use moonkale_core::{NodeKind, Query, SourceFamily};

#[tokio::test]
#[ignore]
async fn open_query_read_and_git_over_the_server() {
    let url = std::env::var("MOONKALE_REMOTE").expect("MOONKALE_REMOTE");
    let token = std::env::var("MOONKALE_TOKEN").ok();
    api::client::connect(&url, token.as_deref(), "test");
    assert!(api::client::active().is_some());

    // Open the server's default root: the folder and its index come back.
    let sources = api::client::open_folder(String::new(), Default::default())
        .await
        .expect("open_folder over the server");
    let folder = sources
        .iter()
        .find(|s| s.descriptor().family == SourceFamily::Folder)
        .expect("a folder source");
    let d = folder.descriptor();
    println!("remote folder: {} ({})", d.display_name, d.id);
    // Browse and read through the remote source.
    let root = folder
        .query(Query::Children(d.root))
        .await
        .expect("children");
    let file = root
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::File && n.native_key.ends_with(".md"))
        .expect("a markdown file at the root");
    let (text, _) = folder.fetch_text(file.id).await.expect("fetch_text");
    assert!(!text.is_empty());
    println!("read {} ({} bytes)", file.native_key, text.len());
    // A second attach by descriptor (another window would do this).
    let again = api::client::attach_source(d.clone()).await.expect("attach");
    assert_eq!(again.id(), d.id);
    // Git and wasm relays answer (an error string is fine on a folder without git/modules).
    let git = api::client::git(
        d.id.as_str().trim_start_matches("folder:").to_string(),
        ui::GitRequest::Status,
    )
    .await;
    println!(
        "git status: {}",
        git.as_ref().map(|_| "ok").unwrap_or("error")
    );
    let wasm = api::client::wasm_list(None).await.expect("wasm list");
    println!("wasm modules on the server: {}", wasm.len());
    api::client::disconnect();
    assert!(api::client::active().is_none());
}
