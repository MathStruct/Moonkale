//! Saves of the folder's settings file never overlap. The first start with a
//! state store saves twice in quick succession (the layout migration); the
//! saves used to race, report "version conflict" (seen on Android,
//! 2026-10-15), lose the newer change, or replace only part of the file.
//! Its own test binary: the state store handle is process-wide.

use dioxus::prelude::*;
use moonkale_core::{SourceError, SourceId};
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use moonkale_state::MemoryStore;
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
            services: &[],
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
async fn overlapping_saves_after_the_migration_do_not_conflict() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    let (ws, _store, path, folder) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let f = folder.clone();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { ws.load_workspace_settings(&f).await });
    });
    settle(&mut dom).await;

    // Two saves started together, before either has written.
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            ws.update_workspace_settings(|f| f.layout = Some("A".into()))
                .await
        });
        spawn(async move {
            ws.update_workspace_settings(|f| {
                f.layout = Some("B".into());
                f.editor.markdown_rich = Some(true);
            })
            .await
        });
    });
    // Poll without parsing a half-written file.
    for _ in 0..50 {
        let read =
            std::fs::read_to_string(path.join(".moonkale/settings.json")).unwrap_or_default();
        if serde_json::from_str::<serde_json::Value>(&read)
            .is_ok_and(|v| v["editor"]["markdown_rich"] == true)
        {
            break;
        }
        settle(&mut dom).await;
    }
    settle(&mut dom).await;
    let status = ws.shell.status.peek().clone();
    assert!(!status.contains("conflict"), "status: {status}");
    let file = on_disk(&path);
    assert!(file.get("layout").is_none(), "{file}");
    assert_eq!(
        file["editor"]["markdown_rich"], true,
        "the newest save wins: {file}"
    );
}
