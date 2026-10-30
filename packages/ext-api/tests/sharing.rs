//! Exercise opt-in metadata against an actual Workspace, not just wire constructors.
use dioxus::prelude::*;
use moonkale_core::{Node, NodeId, NodeKind, SourceId, Version};
use moonkale_ext_api::*;
thread_local! { static WS: std::cell::RefCell<Option<Workspace>> = const { std::cell::RefCell::new(None) }; }
fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Ok(Vec::new()) })
}
fn attach(_: moonkale_core::SourceDescriptor) -> AttachFuture {
    Box::pin(async { Err(moonkale_core::SourceError::NotFound) })
}
fn app() -> Element {
    use_hook(|| {
        let mut ws = Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                attach,
                pick: None,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        });
        let source = SourceId::new("folder:test");
        let id = NodeId::derive(&source, "main.rs");
        let node = Node {
            id,
            source,
            native_key: "main.rs".into(),
            label: "main.rs".into(),
            kind: NodeKind::File,
            content: None,
            version: Version(1),
            props: Default::default(),
        };
        let doc = Signal::new_in_scope(
            Document::new(node, "one\ntwo".into(), Version(1)),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|docs| docs.push((id, doc)));
        ws.docs.active.set(Some(id));
        ws.set_cursor(id, 1, 2);
        ws.set_selection(id, 4, 6);
        WS.with(|slot| *slot.borrow_mut() = Some(ws));
    });
    rsx! {}
}
#[test]
fn off_pointer_only_stop_and_private_paths() {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let mut ws = WS.with(|slot| slot.borrow_mut().take().unwrap());
    dom.in_scope(ScopeId::ROOT, || {
        assert!(ws.my_presence().sharing.is_none());
        assert!(ws.my_presence().line.is_none());
        ws.share_positions(false, true);
        assert!(ws.my_presence().sharing.unwrap().cursor.is_none());
        assert!(ws.my_presence().line.is_none());
        ws.share_positions(true, true);
        let cursor = ws.my_presence().sharing.unwrap().cursor.unwrap();
        assert_eq!(
            (cursor.anchor, cursor.head, cursor.line, cursor.column),
            (4, 6, 1, 2)
        );
        let (_, mut doc) = ws.active_document().unwrap();
        doc.write().node.native_key = ".secrets/token".into();
        assert!(ws.my_presence().sharing.unwrap().cursor.is_none());
        ws.session.presence.set(vec![presence::Member {
            window: "peer".into(),
            name: "Peer".into(),
            active: None,
            line: None,
            sharing: Some(sharing::Position {
                updated: workspace::now_ms(),
                cursor: None,
                pointer: Some(sharing::Pointer {
                    view: "graph".into(),
                    x: 1.0,
                    y: 2.0,
                }),
                agent: Some(("Agent".into(), cursor)),
            }),
        }]);
        let participants = ws.others();
        assert_eq!(participants.len(), 2);
        assert!(participants[0].sharing.as_ref().unwrap().pointer.is_some());
        assert!(
            participants[1].sharing.as_ref().unwrap().pointer.is_none(),
            "an agent cursor must not inherit its host's pointer"
        );
        ws.stop_sharing();
        let state = ws.session.sharing.peek();
        assert!(
            !state.cursor
                && !state.pointer
                && !state.agent_enabled
                && state.position.is_none()
                && state.agent.is_none()
        );
        assert!(ws.my_presence().sharing.is_none());
    });
}
