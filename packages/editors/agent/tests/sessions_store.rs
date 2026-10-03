//! Milestone 18 phase 5.15: with a host store, the Agent panel's local
//! sessions are rows of `agent_sessions` under `folder · "local"`; the
//! session files of an older build are imported once and left alone.

use dioxus::prelude::*;
use moonkale_core::{SourceError, SourceId};
use moonkale_editor_agent::{saved_sessions, sessions_key, Item, SavedSession, SESSIONS_DIR};
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use moonkale_state::{MemoryStore, StateStore, Typed};
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
                host: Some(state),
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
        // Two session files from before the store.
        let local = dir.path().join(SESSIONS_DIR);
        std::fs::create_dir_all(&local).unwrap();
        for (id, created) in [("a", 1), ("b", 2)] {
            let s = SavedSession {
                id: id.into(),
                title: format!("chat {id}"),
                profile: "Default".into(),
                created,
                messages: Vec::new(),
                items: vec![Item::User(format!("hello {id}"))],
                cited: Vec::new(),
            };
            std::fs::write(
                local.join(format!("{id}.json")),
                serde_json::to_string(&s).unwrap(),
            )
            .unwrap();
        }
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

#[tokio::test(flavor = "current_thread")]
async fn local_sessions_live_in_the_host_store() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    let (ws, store, path, folder) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let files = |p: &std::path::Path| std::fs::read_dir(p.join(SESSIONS_DIR)).unwrap().count();
    let listed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    // The first listing imports the files.
    let (f, l) = (folder.clone(), listed.clone());
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { *l.borrow_mut() = saved_sessions(ws, &f).await });
    });
    settle(&mut dom).await;
    assert_eq!(listed.borrow().len(), 2);
    let stored = Typed::new(&*store)
        .scan::<SavedSession>(&sessions_key(&folder))
        .unwrap();
    assert_eq!(
        stored
            .iter()
            .map(|(_, s)| s.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );

    // With the rows there, a deleted file changes nothing: the store answers.
    std::fs::remove_file(path.join(SESSIONS_DIR).join("a.json")).unwrap();
    assert_eq!(files(&path), 1);
    let (f, l) = (folder.clone(), listed.clone());
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { *l.borrow_mut() = saved_sessions(ws, &f).await });
    });
    settle(&mut dom).await;
    assert_eq!(listed.borrow().len(), 2);
    assert!(matches!(&listed.borrow()[0].items[..], [Item::User(t)] if t == "hello a"));
    // Only this folder's rows.
    assert!(store
        .scan(
            moonkale_state::tables::AGENT_SESSIONS,
            sessions_key(&SourceId::new("folder:other")).as_bytes()
        )
        .unwrap()
        .is_empty());
}
