//! Cross-window membership, catalogue isolation and ownership transfer on real Workspaces.
use dioxus::prelude::*;
use moonkale_core::{Node, SourceDescriptor, SourceError};
use moonkale_ext_api::{window_panels::*, *};
use std::{cell::RefCell, rc::Rc, sync::Arc};

thread_local! { static SETUP: RefCell<Option<(Workspace, Workspace)>> = const { RefCell::new(None) }; }
fn open(path: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        Ok(vec![
            Arc::new(moonkale_project_fs::FolderSource::open(path)?)
                as Arc<dyn moonkale_core::Source>,
        ])
    })
}
fn attach(source: SourceDescriptor) -> AttachFuture {
    Box::pin(async move {
        let path = source
            .id
            .as_str()
            .strip_prefix("folder:")
            .ok_or(SourceError::NotFound)?;
        Ok(Arc::new(moonkale_project_fs::FolderSource::open(path)?)
            as Arc<dyn moonkale_core::Source>)
    })
}
fn app() -> Element {
    use_hook(|| {
        let create = || {
            Workspace::new(WorkspaceConfig {
                folders: FolderAccess {
                    open,
                    attach,
                    pick: None,
                    reopen_last: false,
                    openers: &moonkale_core::source::opener::NO_OPENERS,
                },
                persistence: Default::default(),
                processes: Default::default(),
                network: Default::default(),
                runtimes: Default::default(),
                services: &[],
            })
        };
        SETUP.with(|s| *s.borrow_mut() = Some((create(), create())));
    });
    rsx! {}
}
#[derive(Default)]
struct Bus(RefCell<Vec<SessionMessage>>);
impl SessionBus for Bus {
    fn send(&self, message: SessionMessage) {
        self.0.borrow_mut().push(message);
    }
}
async fn deliver(bus: &Bus, ws: Workspace) {
    let messages = std::mem::take(&mut *bus.0.borrow_mut());
    for message in messages {
        ws.handle_message(message).await;
    }
}
async fn run<T: 'static>(
    dom: &mut VirtualDom,
    future: impl std::future::Future<Output = T> + 'static,
) -> T {
    let (tx, mut rx) = futures_channel::oneshot::channel();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            let _ = tx.send(Box::pin(future).await);
        });
    });
    for _ in 0..1000 {
        if let Some(value) = rx.try_recv().unwrap() {
            return value;
        }
        tokio::select! { _ = dom.wait_for_work() => {}, _ = tokio::time::sleep(std::time::Duration::from_millis(2)) => {} }
        dom.render_immediate(&mut dioxus_core::NoOpMutations);
    }
    panic!("window protocol did not settle");
}
fn tab(panel: &str, node: &Node, group: Option<&str>) -> WindowTab {
    WindowTab {
        movable: true,
        panel: panel.into(),
        title: node.label.clone(),
        path: node.native_key.clone(),
        category: "markdown".into(),
        group: group.map(str::to_string),
        dirty: false,
    }
}
#[tokio::test(flavor = "current_thread")]
async fn move_graph_keeps_remote_lead_navigation_marks_and_scope() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let (mut a, mut b) = SETUP.with(|s| s.borrow_mut().take().unwrap());
    run(&mut dom, async move {
        let ab = Rc::new(Bus::default()); let ba = Rc::new(Bus::default());
        a.connect_bus(ab.clone()); b.connect_bus(ba.clone());
        let folder = tempfile::tempdir().unwrap();
        for name in ["Home.md","Alpha.md"] { std::fs::write(folder.path().join(name), format!("# {name}\n")).unwrap(); }
        let source = a.open_folder(folder.path().display().to_string()).await.unwrap();
        let home = a.node_at_path(&source.id,"Home.md").await.unwrap();
        let alpha = a.node_at_path(&source.id,"Alpha.md").await.unwrap();
        let initial = a.open_with_context(home.clone()).await.unwrap();
        let graph = format!("graph:{initial}");
        let lead = a.content_panel(home.id);
        a.link_panel(&graph, &lead, false);
        let group = a.panel_group(&graph).unwrap().id;
        a.set_window_tabs(vec![tab(&lead,&home,Some(&group)), tab(&graph,&home,Some(&group))]);
        a.publish_link_changes(); deliver(&ab,b).await;
        assert_eq!(b.session.remote_tabs.peek()[0].tabs.len(),2);
        assert!(b.links.contents.peek().is_empty()); assert!(b.links.views.peek().is_empty(),"metadata must not instantiate remote panels");
        ba.0.borrow_mut().clear();
        b.publish_link_changes(); assert!(!ba.0.borrow().iter().any(|m| matches!(m,SessionMessage::Scoped{message,..} if matches!(message.as_ref(),SessionMessage::Links{..}))),"received metadata echoed");
        assert!(a.start_drag_from_tab(&format!("wb-tab-{}", graph.replace(':',"-"))));
        let mut wrong = ab.0.borrow().iter().find_map(|m| match m {
            SessionMessage::Scoped { message, .. } => match message.as_ref() { SessionMessage::PanelDrag { transfer, .. } => Some(*transfer.clone()), _ => None }, _ => None
        }).unwrap();
        assert!(wrong.valid()); wrong.node.source = moonkale_core::SourceId::new("wrong-source"); assert!(!wrong.valid());
        deliver(&ab,b).await; assert!(b.session.foreign_drag.peek().is_some());
        b.accept_drop().await.unwrap(); deliver(&ba,a).await;
        assert!(!a.links.views.peek().contains_key(&graph)); assert!(a.links.contents.peek().contains_key(&lead));
        assert!(b.links.views.peek().contains_key(&graph)); assert!(b.links.contents.peek().is_empty());
        assert_eq!(b.panel_group(&graph).unwrap().marker(),a.panel_group(&lead).unwrap().marker());
        a.open_node(alpha.clone()).await.unwrap(); a.move_group_lead(&group,alpha.id); a.publish_link_changes(); deliver(&ab,b).await;
        assert_eq!(b.link_subject(&graph).unwrap().node,alpha.id);
        a.select_linked(&lead,vec!["row-7".into()]); a.publish_link_changes(); deliver(&ab,b).await;
        assert_eq!(b.link_subject(&graph).unwrap().selection,["row-7"]);
        // An ordinary copy in B is not A's conventional linked lead, even
        // though both use the same source node and ordinary panel identity.
        let local_home = b.node_at_path(&source.id, "Home.md").await.unwrap();
        b.open_node(local_home).await.unwrap();
        let copy = b.content_panel(home.id);
        assert!(b.panel_group(&copy).is_none());
        assert_eq!(b.link_subject(&copy).unwrap().node, home.id);
        b.select_linked(&copy, vec!["not-a-member".into()]);
        assert_eq!(b.link_subject(&graph).unwrap().selection, ["row-7"]);
        b.close_node(home.id); b.publish_link_changes(); deliver(&ba,a).await;
        assert!(b.links.views.peek().contains_key(&graph));
        assert!(a.panel_group(&lead).is_some());
        assert!(b.open_node_by_id(home.id).is_some(), "retain graph move metadata, not its closed buffer");
        // A different outer session cannot consume focus, catalogues or groups.
        let before = b.links.groups.peek().clone();
        b.saved_sessions.active.set("different".into());
        a.move_group_lead(&group,home.id); a.publish_link_changes(); deliver(&ab,b).await;
        assert_eq!(*b.links.groups.peek(),before);
        b.saved_sessions.active.set("default".into());
        assert!(a.close_content_panel(&lead)); a.publish_link_changes(); deliver(&ab,b).await;
        assert!(!b.links.views.peek().contains_key(&graph), "closing the remote lead must close its group follower");
    }).await;
}
#[tokio::test(flavor = "current_thread")]
async fn focus_checks_catalogue_and_dirty_moves_are_refused() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let (mut a, b) = SETUP.with(|s| s.borrow_mut().take().unwrap());
    run(&mut dom, async move {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("Note.md"), "text").unwrap();
        let source = a
            .open_folder(folder.path().display().to_string())
            .await
            .unwrap();
        let node = a.node_at_path(&source.id, "Note.md").await.unwrap();
        a.open_node(node.clone()).await.unwrap();
        let panel = a.content_panel(node.id);
        a.set_window_tabs(vec![tab(&panel, &node, None)]);
        let bus = Rc::new(Bus::default());
        a.connect_bus(bus.clone());
        bus.0.borrow_mut().clear();
        let request = |panel| SessionMessage::Scoped {
            session: "default".into(),
            message: Box::new(SessionMessage::FocusPanel {
                from: b.session.window.peek().clone(),
                to: a.session.window.peek().clone(),
                panel,
            }),
        };
        a.handle_message(request("unknown".into())).await;
        assert_eq!(*a.session.focus_requested.peek(), 0);
        a.handle_message(request(panel.clone())).await;
        assert_eq!(*a.session.focus_requested.peek(), 1);
        a.document(node.id).unwrap().write().text = "unsaved".into();
        let mut dirty = tab(&panel, &node, None);
        dirty.dirty = true;
        a.set_window_tabs(vec![dirty]);
        bus.0.borrow_mut().clear();
        assert!(!a.start_drag_from_tab(&format!("wb-tab-{}", panel.replace(':', "-"))));
        assert!(bus.0.borrow().is_empty());
        assert!(a.document(node.id).unwrap().peek().dirty());
    })
    .await;
}
#[test]
fn catalogue_rejects_oversized_labels() {
    let mut tab = WindowTab {
        movable: true,
        panel: "editor:test".into(),
        title: "test".into(),
        path: "test.md".into(),
        category: "markdown".into(),
        group: None,
        dirty: false,
    };
    assert!(tab.valid());
    tab.title = "x".repeat(513);
    assert!(!tab.valid());
}

#[test]
fn browser_wire_uses_bounded_versioned_json_and_new_catalogue_shapes() {
    let text = include_str!("../../../fuzz/corpus/session_bundle/window-tabs.json");
    let parsed = SessionMessage::parse_wire(text).unwrap();
    assert!(
        matches!(parsed, SessionMessage::Scoped { message, .. } if matches!(*message, SessionMessage::WindowTabs { .. }))
    );
    assert!(SessionMessage::parse_wire("{}").is_err());
    assert!(SessionMessage::parse_wire(&" ".repeat(4 * 1024 * 1024 + 1)).is_err());
    assert!(
        SessionMessage::parse_wire(&format!("{}0{}", "[".repeat(200), "]".repeat(200))).is_err()
    );
}
