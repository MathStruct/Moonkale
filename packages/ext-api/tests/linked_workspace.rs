//! Lossless migration and independent working sets under ADR-0025.
use dioxus::prelude::*;
use moonkale_ext_api::*;
use moonkale_state::MemoryStore;
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
async fn independent_group_navigation_pin_unlink_and_shared_content_close() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let (mut ws, _) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    run(&mut dom, async move {
        ws.load_sessions().await.unwrap();
        let folder = tempfile::tempdir().unwrap();
        for name in ["Home.md", "Alpha.md", "Beta.md"] {
            std::fs::write(folder.path().join(name), format!("# {name}\n")).unwrap();
        }
        let source = ws
            .open_folder(folder.path().display().to_string())
            .await
            .unwrap();
        let mut nodes = Vec::new();
        for name in ["Home.md", "Alpha.md", "Beta.md"] {
            nodes.push(ws.node_at_path(&source.id, name).await.unwrap());
        }
        let a = ws.open_with_context(nodes[0].clone()).await.unwrap();
        let b = ws.open_with_context(nodes[1].clone()).await.unwrap();
        let graph_a = format!("graph:{a}");
        let graph_b = format!("graph:{b}");
        let lead_a = format!("lead:{a}");
        let lead_b = format!("lead:{b}");
        let before_b = ws.panel_group(&graph_b).unwrap();
        ws.steer_linked(&graph_a, nodes[2].clone()).await.unwrap();
        assert_eq!(ws.link_subject(&graph_a).unwrap().node, nodes[2].id);
        assert_eq!(ws.panel_group(&graph_b).unwrap(), before_b);
        ws.links.focused.set(Some(graph_a.clone()));
        assert!(ws.navigate_linked(false));
        assert_eq!(ws.link_subject(&graph_a).unwrap().node, nodes[0].id);
        assert!(ws.navigate_linked(true));
        assert_eq!(ws.link_subject(&graph_a).unwrap().node, nodes[2].id);
        // Row/block steering records the selection as well as its owner.
        ws.steer_linked_selection(&graph_a, vec!["block-a".into()]);
        ws.steer_linked_selection(&graph_a, vec!["block-b".into()]);
        assert!(ws.navigate_linked(false));
        assert_eq!(ws.link_subject(&graph_a).unwrap().selection, ["block-a"]);
        assert!(ws.navigate_linked(true));
        assert_eq!(ws.link_subject(&graph_a).unwrap().selection, ["block-b"]);
        ws.link_panel(&graph_a, &lead_a, true);
        ws.steer_linked(&graph_a, nodes[1].clone()).await.unwrap();
        assert_eq!(ws.link_subject(&graph_a).unwrap().node, nodes[2].id);
        assert_eq!(ws.linked_content(&lead_a), Some(nodes[1].id));
        assert_eq!(ws.linked_content(&lead_b), Some(nodes[1].id));
        assert!(ws.close_content_panel(&lead_a));
        assert!(ws.links.views.peek().contains_key(&graph_a));
        assert!(ws.document(nodes[1].id).is_some());
        ws.unlink_panel(&graph_a);
        assert!(ws.panel_group(&graph_a).is_none());
        assert_eq!(ws.link_subject(&graph_a).unwrap().node, nodes[2].id);
        assert!(ws.links.views.peek().contains_key(&graph_a));
        ws.close_linked_panel(&graph_a);
        assert!(!ws.links.views.peek().contains_key(&graph_a));
        assert!(ws.close_content_panel(&lead_b));
        assert!(!ws.links.views.peek().contains_key(&graph_b));
        assert!(ws.document(nodes[1].id).is_none());
    })
    .await;
}
