//! Editor sessions must outlive the component that first asks for them.
use dioxus::prelude::*;
use moonkale_core::{Node, NodeId, NodeKind, SourceError, SourceId, Version};
use moonkale_ext_api::{
    document::Document,
    editor::{EditBatch, TextChange, Utf16Selection},
    AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig,
};

fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: moonkale_core::SourceDescriptor) -> AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
thread_local! {
    static SETUP: std::cell::RefCell<Option<(Workspace, Signal<bool>)>> = const { std::cell::RefCell::new(None) };
}
fn node() -> Node {
    Node {
        id: NodeId::fresh("session-remount"),
        source: SourceId::new("test"),
        kind: NodeKind::File,
        label: "test.rs".into(),
        native_key: "test.rs".into(),
        props: Default::default(),
        content: None,
        version: Version::default(),
    }
}
#[component]
fn App() -> Element {
    let ws = use_hook(|| {
        let mut ws = Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                pick: None,
                attach,
                reopen_last: false,
                openers: &moonkale_core::source::opener::NO_OPENERS,
            },
            processes: Default::default(),
            persistence: Default::default(),
            network: Default::default(),
            runtimes: Default::default(),
            services: &[],
        });
        let n = node();
        let doc = Signal::new_in_scope(
            Document::new(n.clone(), "abc".into(), n.version),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|docs| docs.push((n.id, doc)));
        ws
    });
    let mounted = use_signal(|| true);
    use_hook(move || SETUP.with(|setup| *setup.borrow_mut() = Some((ws, mounted))));
    rsx! { if mounted() { Panel { ws } } }
}
#[component]
fn Panel(ws: Workspace) -> Element {
    // Deliberately first create the session in a child scope.
    let session = ws.editor_session(node().id).unwrap();
    let _state = ws
        .editor_view_state(node().id, || (7usize, "caret".to_string()))
        .unwrap();
    rsx! { div { "{session.read().snapshot().text}" } }
}

#[test]
fn session_history_survives_unmount_and_editor_switch_and_close_discards_it() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let (mut ws, mut mounted) = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
    let id = node().id;
    let mut session = dom.in_scope(ScopeId::ROOT, || ws.editor_session(id).unwrap());
    let view = dom.in_scope(ScopeId::ROOT, || {
        ws.editor_view_state(id, || unreachable!()).unwrap()
    });
    let view: Signal<(usize, String)> = view;
    let original = session.peek().snapshot().clone();
    session.with_mut(|state| {
        state
            .apply(&EditBatch {
                node: id,
                base_revision: original.revision,
                changes: vec![TextChange {
                    range: 1..2,
                    removed_text: "b".into(),
                    inserted_text: "😀".into(),
                }],
                selection: Some(Utf16Selection { anchor: 3, head: 1 }),
            })
            .unwrap();
    });
    let edited = session.peek().snapshot().clone();
    ws.document(id).unwrap().write().text = edited.text.clone();
    mounted.set(false);
    dom.render_immediate(&mut dioxus_core::NoOpMutations);
    ws.choose_editor(id, "codemirror");
    assert_eq!(session.peek().snapshot(), &edited);
    mounted.set(true);
    dom.render_immediate(&mut dioxus_core::NoOpMutations);
    let mut remounted = dom.in_scope(ScopeId::ROOT, || ws.editor_session(id).unwrap());
    assert_eq!(session, remounted);
    let retained: Signal<(usize, String)> = dom.in_scope(ScopeId::ROOT, || {
        ws.editor_view_state(id, || unreachable!()).unwrap()
    });
    assert_eq!(view, retained);
    assert_eq!(*retained.peek(), (7, "caret".into()));
    assert_eq!(remounted.peek().snapshot(), &edited);
    assert_eq!(
        remounted
            .with_mut(|state| state.undo().unwrap().unwrap())
            .text,
        "abc"
    );
    assert_eq!(
        remounted
            .with_mut(|state| state.redo().unwrap().unwrap())
            .text,
        "a😀c"
    );
    mounted.set(false);
    dom.render_immediate(&mut dioxus_core::NoOpMutations);
    let nested: Signal<Signal<String>> = dom.in_scope(ScopeId::ROOT, || {
        ws.editor_view_state_with_cleanup(
            id,
            || Signal::new_in_scope("nested model".to_string(), ScopeId::ROOT),
            |signal| signal.manually_drop(),
        )
        .unwrap()
    });
    let inner = *nested.peek();
    ws.close_node(id);
    assert!(
        inner.try_peek().is_err(),
        "nested view resources must be freed"
    );
    assert!(nested.try_peek().is_err());
    assert!(ws.docs.editor_sessions.peek().get(&id).is_none());
    assert!(!ws.docs.editor_views.peek().contains_key(&id));
    assert!(
        session.try_peek().is_err(),
        "closed session must release its root allocation"
    );
    assert!(
        view.try_peek().is_err(),
        "closed view must release its root allocation"
    );
    assert!(dom
        .in_scope(ScopeId::ROOT, || ws.editor_session(id))
        .is_none());
    dom.in_scope(ScopeId::ROOT, || {
        let n = node();
        let doc = Signal::new_in_scope(
            Document::new(n.clone(), "reopened".into(), n.version),
            ScopeId::ROOT,
        );
        ws.docs.open.with_mut(|docs| docs.push((id, doc)));
        let mut fresh = ws.editor_session(id).unwrap();
        assert_ne!(fresh, session);
        assert_eq!(fresh.peek().snapshot().text, "reopened");
        assert!(fresh.with_mut(|state| state.undo().unwrap()).is_none());
    });
}
