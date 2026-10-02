//! ADR-0014: with a state store, a folder's layout and open documents live
//! in the store and the folder's `.moonkale/settings.json` keeps only what
//! the folder shares. A file from before the store hands its layout over on
//! the first load and loses it on the next save.

use dioxus::prelude::*;
use moonkale_core::{SourceError, SourceId};
use moonkale_ext_api::settings::LayoutRecord;
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use moonkale_state::{Key, MemoryStore, StateStore, Typed};
use std::sync::Arc;

fn open_folder(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

fn attach(_: moonkale_core::SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

type Setup = (Workspace, Arc<MemoryStore>, std::path::PathBuf, SourceId);

thread_local! {
    static SETUP: std::cell::RefCell<Option<Setup>> = const { std::cell::RefCell::new(None) };
    static DIR: std::cell::RefCell<Option<tempfile::TempDir>> = const { std::cell::RefCell::new(None) };
}

#[component]
fn App() -> Element {
    let store = use_hook(|| Arc::new(MemoryStore::new()));
    let state = moonkale_ext_api::local_state(store.clone());
    let mut ws = use_context_provider(|| {
        Workspace::new(WorkspaceConfig {
            folders: moonkale_ext_api::FolderAccess {
                open: open_folder,
                pick: None,
                attach,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: moonkale_ext_api::Persistence {
                state: Some(state),
                ..Default::default()
            },
            network: Default::default(),
            runtimes: Default::default(),
        })
    });
    use_hook(move || {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".moonkale")).unwrap();
        // A settings file from before the store: shared settings and a layout.
        std::fs::write(
            dir.path().join(".moonkale/settings.json"),
            r#"{"version":1,"editor":{"markdown_rich":false},"layout":"OLD","open_documents":["a.md"],"active_document":"a.md"}"#,
        )
        .unwrap();
        let folder = moonkale_project_fs::FolderSource::open(dir.path()).unwrap();
        let d = ws.add_source(Arc::new(folder));
        let path = dir.path().to_path_buf();
        DIR.with(|s| *s.borrow_mut() = Some(dir));
        SETUP.with(|s| *s.borrow_mut() = Some((ws, store, path, d.id)));
    });
    rsx! { div {} }
}

async fn settle(dom: &mut VirtualDom) {
    for _ in 0..30 {
        tokio::select! {
            _ = dom.wait_for_work() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }
        dom.render_immediate(&mut dioxus_core::NoOpMutations);
    }
}

fn on_disk(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path.join(".moonkale/settings.json")).unwrap())
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn layouts_live_in_the_store_and_the_folder_file_keeps_what_it_shares() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    let (ws, store, path, folder) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let key = Key::new().str(folder.as_str());
    let record = |store: &MemoryStore| Typed::new(store).get::<LayoutRecord>(&key).unwrap();

    // Load: the old file's layout is handed to the store.
    let f = folder.clone();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { ws.load_workspace_settings(&f).await });
    });
    settle(&mut dom).await;
    assert_eq!(ws.settings.workspace.peek().layout.as_deref(), Some("OLD"));
    assert_eq!(
        record(&store).and_then(|r| r.layout).as_deref(),
        Some("OLD")
    );

    // Save a layout change: it goes to the store; the folder file loses its
    // layout part and keeps the shared settings.
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            ws.update_workspace_settings(|f| f.layout = Some("NEW".into()))
                .await
        });
    });
    settle(&mut dom).await;
    assert_eq!(
        record(&store).and_then(|r| r.layout).as_deref(),
        Some("NEW")
    );
    let file = on_disk(&path);
    assert!(
        file.get("layout").is_none() && file.get("open_documents").is_none(),
        "{file}"
    );
    assert_eq!(
        file["editor"]["markdown_rich"], false,
        "shared settings stay in the folder"
    );

    // Another layout change does not touch the folder file at all.
    let before = std::fs::read_to_string(path.join(".moonkale/settings.json")).unwrap();
    let mtime = std::fs::metadata(path.join(".moonkale/settings.json"))
        .unwrap()
        .modified()
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            ws.update_workspace_settings(|f| f.layout = Some("NEWER".into()))
                .await
        });
    });
    settle(&mut dom).await;
    assert_eq!(
        record(&store).and_then(|r| r.layout).as_deref(),
        Some("NEWER")
    );
    assert_eq!(
        std::fs::read_to_string(path.join(".moonkale/settings.json")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::metadata(path.join(".moonkale/settings.json"))
            .unwrap()
            .modified()
            .unwrap(),
        mtime
    );

    // A fresh load (a restart) gets the layout back from the store.
    dom.in_scope(ScopeId::ROOT, || {
        let f = folder.clone();
        spawn(async move { ws.load_workspace_settings(&f).await });
    });
    settle(&mut dom).await;
    assert_eq!(
        ws.settings.workspace.peek().layout.as_deref(),
        Some("NEWER")
    );
    assert!(
        store
            .scan(moonkale_state::tables::LAYOUT, b"")
            .unwrap()
            .len()
            == 1
    );
}
