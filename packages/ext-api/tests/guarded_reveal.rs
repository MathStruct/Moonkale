//! A stale asynchronous navigation must not create a document or change focus.
use dioxus::prelude::*;
use moonkale_core::*;
use moonkale_ext_api::{
    AttachFuture, FolderAccess, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: SourceDescriptor) -> AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
struct File {
    valid: AtomicBool,
    invalidate: AtomicBool,
}
impl File {
    fn node() -> Node {
        Node {
            id: NodeId::derive(&SourceId::new("guarded"), "target.rs"),
            source: SourceId::new("guarded"),
            kind: NodeKind::File,
            label: "target.rs".into(),
            native_key: "target.rs".into(),
            content: None,
            props: Default::default(),
            version: Version::default(),
        }
    }
}
#[moonkale_core::async_trait]
impl Source for File {
    fn id(&self) -> SourceId {
        SourceId::new("guarded")
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id(),
            root: Self::node().id,
            display_name: "guarded".into(),
            family: SourceFamily::Folder,
            capabilities: Default::default(),
        }
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        Ok(QueryResult::default())
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        tokio::task::yield_now().await;
        if self.invalidate.load(Ordering::SeqCst) {
            self.valid.store(false, Ordering::SeqCst);
        }
        Ok(("target".into(), Version::default()))
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::NotFound)
    }
}
thread_local! { static SETUP: std::cell::RefCell<Option<(Workspace, Arc<File>)>> = const { std::cell::RefCell::new(None) }; }
struct Index;
#[moonkale_core::async_trait]
impl Source for Index {
    fn id(&self) -> SourceId {
        SourceId::new("index:guarded")
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id(),
            root: NodeId::derive(&self.id(), "root"),
            display_name: "index".into(),
            family: SourceFamily::Index,
            capabilities: Default::default(),
        }
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        Ok(QueryResult::single(File::node()))
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::NotFound)
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::NotFound)
    }
}
#[component]
fn App() -> Element {
    use_hook(|| {
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
        let file = Arc::new(File {
            valid: AtomicBool::new(true),
            invalidate: AtomicBool::new(true),
        });
        ws.add_source(file.clone());
        SETUP.with(|setup| *setup.borrow_mut() = Some((ws, file)));
    });
    rsx! { div {} }
}
#[test]
fn guard_is_rechecked_after_loading_before_opening_and_revealing() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let (ws, file) = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        let opened = runtime
            .block_on(ws.reveal_guarded(File::node(), 0, 2, || file.valid.load(Ordering::SeqCst)))
            .unwrap();
        assert!(!opened);
        assert!(ws.docs.open.peek().is_empty());
        assert!(ws.docs.active.peek().is_none());
        assert!(ws.docs.reveal.peek().is_none());
        file.valid.store(true, Ordering::SeqCst);
        file.invalidate.store(false, Ordering::SeqCst);
        assert!(runtime
            .block_on(ws.reveal_guarded(File::node(), 0, 2, || file.valid.load(Ordering::SeqCst)))
            .unwrap());
        assert_eq!(ws.docs.open.peek().len(), 1);
        assert_eq!(*ws.docs.active.peek(), Some(File::node().id));
        assert_eq!(ws.docs.reveal.peek().unwrap().col, 2);
        assert_eq!(ws.document(File::node().id).unwrap().peek().text, "target");
    });
}

#[test]
fn wiki_follow_rechecks_guard_after_loading_and_keeps_resolved_source() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let (mut ws, file) = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        ws.add_source(Arc::new(Index));
        let result = runtime
            .block_on(ws.follow_wiki_guarded(&File::node(), "target", false, || {
                file.valid.load(Ordering::SeqCst)
            }))
            .unwrap();
        assert!(result.is_none());
        assert!(ws.docs.open.peek().is_empty());
        assert!(ws.docs.active.peek().is_none());
        file.valid.store(true, Ordering::SeqCst);
        file.invalidate.store(false, Ordering::SeqCst);
        let result = runtime
            .block_on(ws.follow_wiki(&File::node(), "target", false))
            .unwrap();
        assert_eq!(result.source, File::node().source);
        assert_eq!(ws.document(result.id).unwrap().peek().text, "target");
    });
}

#[test]
fn activation_clears_previous_context_and_close_preserves_reveal_ordering() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let (mut ws, file) = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
    file.invalidate.store(false, Ordering::SeqCst);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        runtime.block_on(ws.reveal(File::node(), 0, 2)).unwrap();
        let sequence = ws.docs.reveal.peek().unwrap().seq;
        ws.set_cursor(File::node().id, 0, 2);
        ws.set_selection(File::node().id, 0, 2);
        let mut other = File::node();
        other.native_key = "other.rs".into();
        other.id = NodeId::derive(&other.source, &other.native_key);
        runtime.block_on(ws.open_node(other.clone())).unwrap();
        assert!(ws.docs.cursor.peek().is_none());
        assert!(ws.docs.cursor_line.peek().is_none());
        assert!(ws.docs.selection.peek().is_none());
        ws.close_node(File::node().id);
        assert!(ws.docs.reveal.peek().is_none());
        runtime.block_on(ws.reveal(other, 0, 1)).unwrap();
        assert!(ws.docs.reveal.peek().unwrap().seq > sequence);
    });
}

#[test]
fn unsaved_documents_cannot_be_offered_or_closed_by_move_acknowledgements() {
    use moonkale_ext_api::{SessionBus, SessionMessage, WindowId};
    use std::{cell::RefCell, rc::Rc};
    struct Bus(RefCell<Vec<SessionMessage>>);
    impl SessionBus for Bus {
        fn send(&self, message: SessionMessage) {
            self.0.borrow_mut().push(message);
        }
    }
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    let (mut ws, file) = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
    file.invalidate.store(false, Ordering::SeqCst);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        runtime.block_on(ws.open_node(File::node())).unwrap();
        let bus = Rc::new(Bus(RefCell::new(Vec::new())));
        ws.connect_bus(bus.clone());
        let tab = format!("wb-tab-{}", File::node().id);
        assert!(ws.start_drag_from_tab(&tab));
        ws.document(File::node().id)
            .unwrap()
            .write()
            .text
            .push_str(" unsaved");
        bus.0.borrow_mut().clear();
        assert!(!ws.start_drag_from_tab(&tab));
        assert!(!bus
            .0
            .borrow()
            .iter()
            .any(|message| matches!(message, SessionMessage::DragStarted { .. })));
        let from = ws.session.window.peek().clone();
        runtime.block_on(ws.handle_message(SessionMessage::Moved {
            node: File::node().id,
            from,
            to: WindowId("peer".into()),
        }));
        assert_eq!(
            ws.document(File::node().id).unwrap().peek().text,
            "target unsaved"
        );
    });
}
