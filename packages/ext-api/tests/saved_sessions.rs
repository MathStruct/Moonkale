//! Lossless migration and independent working sets under ADR-0025.
use dioxus::prelude::*;
use moonkale_ext_api::{settings::LayoutRecord, *};
use moonkale_state::{Key, MemoryStore, StateStore, Typed};
use std::sync::Arc;

thread_local! { static SETUP: std::cell::RefCell<Option<(Workspace, Arc<MemoryStore>)>> = const { std::cell::RefCell::new(None) }; }
fn open(path: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        Ok(vec![
            Arc::new(moonkale_project_fs::FolderSource::open(path)?)
                as Arc<dyn moonkale_core::Source>,
        ])
    })
}
fn attach(_: moonkale_core::SourceDescriptor) -> AttachFuture {
    Box::pin(async { Err(moonkale_core::SourceError::NotFound) })
}
fn app() -> Element {
    use_hook(|| {
        let store = Arc::new(MemoryStore::new());
        let state = local_state(store.clone());
        let ws = Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                attach,
                pick: None,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            persistence: Persistence {
                state: Some(state),
                host: Some(state),
                ..Default::default()
            },
            processes: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        });
        SETUP.with(|s| *s.borrow_mut() = Some((ws, store)));
    });
    rsx! {div{}}
}
async fn run<T: 'static>(
    dom: &mut VirtualDom,
    future: impl std::future::Future<Output = T> + 'static,
) -> T {
    let future = Box::pin(future);
    let (tx, mut rx) = futures_channel::oneshot::channel();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            let _ = tx.send(future.await);
        });
    });
    for _ in 0..1000 {
        if let Some(value) = rx.try_recv().unwrap() {
            return value;
        }
        tokio::select! { _ = dom.wait_for_work() => {}, _ = tokio::time::sleep(std::time::Duration::from_millis(2)) => {} }
        dom.render_immediate(&mut dioxus_core::NoOpMutations);
    }
    panic!("session operation did not settle");
}
#[tokio::test(flavor = "current_thread")]
async fn migration_and_two_dirty_sessions_keep_work_and_scope() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let (ws, store) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let legacy = LayoutRecord {
        layout: Some("legacy-layout".into()),
        open_documents: vec!["old.rs".into()],
        active_document: Some("old.rs".into()),
    };
    Typed::new(store.as_ref())
        .put(&Key::new().str("folder:old"), &legacy)
        .unwrap();
    run(&mut dom, ws.load_sessions()).await.unwrap();
    assert_eq!(
        Typed::new(store.as_ref())
            .get::<LayoutRecord>(&Key::new().str("default").str("folder:old"))
            .unwrap(),
        Some(legacy)
    );
    assert!(store
        .get("layout", Key::new().str("folder:old").as_bytes())
        .unwrap()
        .is_none());
    run(&mut dom, ws.load_sessions()).await.unwrap();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for dir in [&a, &b] {
        std::fs::write(dir.path().join("buffer.txt"), "base").unwrap();
    }
    let source = run(&mut dom, ws.open_folder(a.path().display().to_string()))
        .await
        .unwrap();
    let node = run(&mut dom, async move {
        ws.node_at_path(&source.id, "buffer.txt").await.unwrap()
    })
    .await;
    run(&mut dom, ws.open_node(node.clone())).await.unwrap();
    let mut doc_a = ws.document(node.id).unwrap();
    doc_a.write().text = "dirty A".into();
    run(
        &mut dom,
        ws.update_session_settings(|s| s.editor.wrap = Some(true)),
    )
    .await;
    // A scoped window from another outer session cannot join this one.
    run(
        &mut dom,
        ws.handle_message(SessionMessage::Scoped {
            session: "unrelated".into(),
            message: Box::new(SessionMessage::Welcome {
                from: WindowId("foreign".into()),
            }),
        }),
    )
    .await;
    assert!(ws.session.peers.peek().is_empty());
    let second = run(&mut dom, ws.new_session("B".into())).await.unwrap();
    // A late conversation write must keep the captured Default owner.
    let conversation = LayoutRecord {
        layout: Some("late conversation".into()),
        ..Default::default()
    };
    let key = Key::new().str("folder:old").str("local").str("late");
    let batch = moonkale_state::Batch::new().put(
        moonkale_state::tables::AGENT_SESSIONS,
        key.clone(),
        moonkale_state::encode(&conversation),
    );
    run(&mut dom, async move {
        ws.host_write_in_session("default", batch).await
    })
    .await
    .unwrap();
    assert!(store
        .get(
            moonkale_state::tables::AGENT_SESSIONS,
            Key::new()
                .str("folder:old")
                .str("local")
                .str("$session")
                .str("default")
                .str("late")
                .as_bytes()
        )
        .unwrap()
        .is_some());
    assert!(store
        .get(
            moonkale_state::tables::AGENT_SESSIONS,
            Key::new()
                .str("folder:old")
                .str("local")
                .str("$session")
                .str(&second)
                .str("late")
                .as_bytes()
        )
        .unwrap()
        .is_none());
    assert!(ws.docs.open.peek().is_empty());
    assert!(!ws.settings.resolved.peek().editor.wrap);
    let source = run(&mut dom, ws.open_folder(b.path().display().to_string()))
        .await
        .unwrap();
    let node = run(&mut dom, async move {
        ws.node_at_path(&source.id, "buffer.txt").await.unwrap()
    })
    .await;
    run(&mut dom, ws.open_node(node.clone())).await.unwrap();
    let mut doc_b = ws.document(node.id).unwrap();
    doc_b.write().text = "dirty B".into();
    let mut view = node.clone();
    view.id = moonkale_core::NodeId::fresh("table-session");
    view.kind = moonkale_core::NodeKind::Table;
    view.native_key = "table".into();
    run(&mut dom, ws.open_node(view.clone())).await.unwrap();
    run(&mut dom, ws.switch_session("default".into()))
        .await
        .unwrap();
    assert_eq!(ws.docs.open.peek()[0].1, doc_a);
    assert!(ws.docs.views.peek().is_empty());
    assert_eq!(doc_a.peek().text, "dirty A");
    assert_eq!(doc_b.peek().text, "dirty B");
    assert!(ws.settings.resolved.peek().editor.wrap);
    run(&mut dom, ws.switch_session(second.clone()))
        .await
        .unwrap();
    assert_eq!(ws.docs.open.peek()[0].1, doc_b);
    assert_eq!(ws.docs.views.peek().as_slice(), &[view]);
    assert_eq!(run(&mut dom, ws.recovery_snapshots()).await.len(), 1);
    let mut bundle: serde_json::Value =
        serde_json::from_str(&ws.export_session().unwrap()).unwrap();
    bundle["settings"]["policy"] = serde_json::json!({"allow_writes":true});
    let imported = run(&mut dom, async move {
        ws.import_session(&bundle.to_string()).await
    })
    .await
    .unwrap();
    assert_ne!(imported, second);
    assert_ne!(
        ws.saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == imported)
            .unwrap()
            .settings
            .policy
            .allow_writes,
        Some(true)
    );
    run(&mut dom, async move { ws.delete_session(&second).await })
        .await
        .unwrap();
    assert_eq!(*ws.saved_sessions.active.peek(), "default");
    assert_eq!(
        std::fs::read_to_string(a.path().join("buffer.txt")).unwrap(),
        "base"
    );
    assert_eq!(
        std::fs::read_to_string(b.path().join("buffer.txt")).unwrap(),
        "base"
    );
    // Browser-style startup does not reopen Default. Its metadata survives a
    // periodic tick and an explicit switch can still restore the saved folder.
    let saved_members = ws
        .saved_sessions
        .list
        .peek()
        .iter()
        .find(|s| s.id == "default")
        .unwrap()
        .sources
        .clone();
    let mut scope = ws.saved_sessions.settings;
    scope.set(SettingsFile::new());
    run(&mut dom, ws.load_sessions()).await.unwrap();
    assert!(ws.settings.resolved.peek().editor.wrap);
    run(&mut dom, ws.persist_session()).await.unwrap();
    assert_eq!(
        ws.saved_sessions
            .list
            .peek()
            .iter()
            .find(|s| s.id == "default")
            .unwrap()
            .sources,
        saved_members
    );
    // A clean restored tab with an unanswered recovery offer must not discard
    // that older dirty snapshot merely because the user switches away.
    let base = doc_a.peek().saved.clone();
    doc_a.write().text = base;
    let temporary = run(&mut dom, ws.new_session("Temporary".into()))
        .await
        .unwrap();
    assert_ne!(temporary, "default");
    run(&mut dom, ws.switch_session("default".into()))
        .await
        .unwrap();
    assert!(run(&mut dom, ws.recovery_snapshots())
        .await
        .iter()
        .any(|snapshot| snapshot.text == "dirty A"));
}
