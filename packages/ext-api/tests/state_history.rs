//! Phase 5.3: with a host store, a folder's entity log is one stored row per
//! event. A `.moonkale/history.jsonl` from before is imported once and no
//! longer written; compaction swaps the folded rows for the snapshot in one
//! batch; a restart reads the log back from the store.

use dioxus::prelude::*;
use moonkale_core::{EntityLog, Event, EventKind, NodeId, SourceError, SourceId};
use moonkale_ext_api::workspace::EventRecord;
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use moonkale_state::{Key, MemoryStore, Typed};
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
        // A log from before the store: three events in the folder.
        let src = SourceId::new("folder:old");
        let mut old = EntityLog::new();
        for (i, key) in ["a.md", "b.md", "c.md"].iter().enumerate() {
            old.append(Event::new(
                1_000 + i as u64,
                "user:test",
                EventKind::Remove {
                    node: NodeId::derive(&src, key),
                },
            ));
        }
        std::fs::write(dir.path().join(".moonkale/history.jsonl"), old.to_jsonl()).unwrap();
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

fn rows(store: &MemoryStore, folder: &SourceId) -> Vec<Event> {
    Typed::new(store)
        .scan::<EventRecord>(&Key::new().str(folder.as_str()))
        .unwrap()
        .into_iter()
        .map(|(_, EventRecord(e))| e)
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn the_log_lives_in_the_host_store_one_row_per_event() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    let (mut ws, store, path, folder) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let file = path.join(".moonkale/history.jsonl");
    let old_file = std::fs::read_to_string(&file).unwrap();

    // Load: the old file is imported, row for row, in log order.
    let f = folder.clone();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { ws.load_workspace_settings(&f).await });
    });
    settle(&mut dom).await;
    assert_eq!(ws.history.log.peek().len(), 3);
    assert_eq!(&rows(&store, &folder)[..], ws.history.log.peek().events());

    // An edit appends one row; the old file is left alone.
    let src = SourceId::new("folder:old");
    dom.in_scope(ScopeId::ROOT, || {
        ws.record(EventKind::Remove {
            node: NodeId::derive(&src, "d.md"),
        });
    });
    settle(&mut dom).await;
    assert_eq!(rows(&store, &folder).len(), 4);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), old_file);

    // Compaction: three folded events become one snapshot, in one batch.
    dom.in_scope(ScopeId::ROOT, || assert_eq!(ws.compact_history(1), 3));
    settle(&mut dom).await;
    let stored = rows(&store, &folder);
    assert_eq!(&stored[..], ws.history.log.peek().events());
    assert_eq!(stored.len(), 2);
    assert!(matches!(
        stored[0].kind,
        EventKind::Snapshot { folded: 3, .. }
    ));

    // A restart reads the store, not the file.
    let before = ws.history.log.peek().clone();
    ws.history.log.set(EntityLog::new());
    let f = folder.clone();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move { ws.load_workspace_settings(&f).await });
    });
    settle(&mut dom).await;
    assert_eq!(*ws.history.log.peek(), before);
    // Rows are per folder: another folder's prefix sees none of them.
    assert!(rows(&store, &SourceId::new("folder:other")).is_empty());
}
