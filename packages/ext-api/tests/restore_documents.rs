//! A delayed saved-tab load must not undo foreground navigation.
use dioxus::prelude::*;
use futures_util::FutureExt;
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
struct Files {
    pause: AtomicBool,
    started: AtomicBool,
    release: tokio::sync::Notify,
}
impl Files {
    fn folder() -> SourceId {
        SourceId::new("folder:restore")
    }
    fn node(path: &str) -> Node {
        Node {
            id: NodeId::derive(&Self::folder(), path),
            source: Self::folder(),
            kind: NodeKind::File,
            label: path.into(),
            native_key: path.into(),
            content: None,
            version: Version::default(),
            props: Default::default(),
        }
    }
}
#[async_trait]
impl Source for Files {
    fn id(&self) -> SourceId {
        Self::folder()
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id(),
            root: Self::node("root").id,
            display_name: "restore".into(),
            family: SourceFamily::Folder,
            capabilities: Default::default(),
        }
    }
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Text { text, .. }
                if matches!(text.as_str(), "saved.md" | "next.md" | "user.md") =>
            {
                Ok(QueryResult::single(Self::node(&text)))
            }
            _ => Ok(QueryResult::default()),
        }
    }
    async fn fetch_text(&self, id: NodeId) -> Result<(String, Version), SourceError> {
        if id == Self::node("saved.md").id && self.pause.load(Ordering::SeqCst) {
            self.started.store(true, Ordering::SeqCst);
            self.release.notified().await;
        }
        Ok(("# body".into(), Version::default()))
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::NotFound)
    }
}
thread_local! { static SETUP: std::cell::RefCell<Option<(Workspace, Arc<Files>)>> = const { std::cell::RefCell::new(None) }; }
fn app() -> Element {
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
        let files = Arc::new(Files {
            pause: AtomicBool::new(true),
            started: AtomicBool::new(false),
            release: tokio::sync::Notify::new(),
        });
        ws.add_source(files.clone());
        ws.settings.folder.set(Some(Files::folder()));
        SETUP.with(|s| *s.borrow_mut() = Some((ws, files)));
    });
    rsx! { div {} }
}
fn scenario(action: impl FnOnce(Workspace, Arc<Files>)) {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let (ws, files) = SETUP.with(|s| s.borrow_mut().take().unwrap());
    dom.in_scope(ScopeId::ROOT, || action(ws, files));
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}
fn keys() -> Vec<String> {
    vec!["saved.md".into(), "next.md".into()]
}
#[test]
fn background_restore_activates_only_the_saved_tab_after_loading() {
    scenario(|ws, files| {
        runtime().block_on(async {
            let restore = ws.restore_documents(Files::folder(), keys(), Some("saved.md".into()));
            let mut restore = std::pin::pin!(restore);
            assert!(restore.as_mut().now_or_never().is_none());
            assert!(files.started.load(Ordering::SeqCst));
            assert!(ws.docs.active.peek().is_none());
            files.release.notify_one();
            restore.await;
            assert_eq!(ws.docs.open.peek().len(), 2);
            assert_eq!(*ws.docs.active.peek(), Some(Files::node("saved.md").id));
        })
    });
}
#[test]
fn foreground_open_wins_and_restoration_keeps_its_unsaved_text_and_selection() {
    scenario(|mut ws, files| {
        runtime().block_on(async {
            let restore = ws.restore_documents(Files::folder(), keys(), Some("saved.md".into()));
            let mut restore = std::pin::pin!(restore);
            assert!(restore.as_mut().now_or_never().is_none());
            let user = Files::node("user.md");
            ws.open_node(user.clone()).await.unwrap();
            ws.document(user.id).unwrap().write().text = "unsaved".into();
            ws.docs.selection.set(Some((2, 4)));
            files.release.notify_one();
            restore.await;
            assert_eq!(ws.docs.open.peek().len(), 3);
            assert_eq!(*ws.docs.active.peek(), Some(user.id));
            assert_eq!(*ws.docs.selection.peek(), Some((2, 4)));
            assert_eq!(ws.document(user.id).unwrap().peek().text, "unsaved");
        })
    });
}
#[test]
fn a_folder_switch_discards_a_delayed_restore() {
    scenario(|mut ws, files| {
        runtime().block_on(async {
            let restore = ws.restore_documents(Files::folder(), keys(), Some("saved.md".into()));
            let mut restore = std::pin::pin!(restore);
            assert!(restore.as_mut().now_or_never().is_none());
            ws.settings.folder.set(Some(SourceId::new("folder:other")));
            files.release.notify_one();
            restore.await;
            assert!(ws.docs.open.peek().is_empty());
            assert!(ws.docs.active.peek().is_none());
        })
    });
}
#[test]
fn already_open_documents_keep_their_active_tab_and_edits() {
    scenario(|mut ws, files| {
        runtime().block_on(async {
            files.pause.store(false, Ordering::SeqCst);
            let user = Files::node("saved.md");
            ws.open_node(user.clone()).await.unwrap();
            ws.document(user.id).unwrap().write().text = "unsaved".into();
            ws.docs.selection.set(Some((1, 3)));
            ws.restore_documents(Files::folder(), keys(), Some("next.md".into()))
                .await;
            assert_eq!(ws.docs.open.peek().len(), 2);
            assert_eq!(*ws.docs.active.peek(), Some(user.id));
            assert_eq!(*ws.docs.selection.peek(), Some((1, 3)));
            assert_eq!(ws.document(user.id).unwrap().peek().text, "unsaved");
        })
    });
}

#[test]
fn choosing_an_already_restored_tab_wins_over_later_saved_activation() {
    scenario(|ws, files| {
        runtime().block_on(async {
            let restore = ws.restore_documents(
                Files::folder(),
                vec!["next.md".into(), "saved.md".into()],
                Some("saved.md".into()),
            );
            let mut restore = std::pin::pin!(restore);
            assert!(restore.as_mut().now_or_never().is_none());
            assert!(ws.document(Files::node("next.md").id).is_some());
            assert!(ws.docs.active.peek().is_none());
            ws.open_node(Files::node("next.md")).await.unwrap();
            files.release.notify_one();
            restore.await;
            assert_eq!(*ws.docs.active.peek(), Some(Files::node("next.md").id));
            assert_eq!(ws.docs.open.peek().len(), 2);
        })
    });
}

#[test]
fn a_foreground_request_cancels_saved_focus_before_its_fetch_finishes() {
    scenario(|ws, files| {
        runtime().block_on(async {
            let restore = ws.restore_documents(Files::folder(), keys(), Some("next.md".into()));
            let mut restore = std::pin::pin!(restore);
            assert!(restore.as_mut().now_or_never().is_none());
            let foreground = ws.open_node(Files::node("saved.md"));
            let mut foreground = std::pin::pin!(foreground);
            assert!(foreground.as_mut().now_or_never().is_none());
            files.release.notify_waiters();
            restore.await;
            assert!(ws.docs.active.peek().is_none());
            foreground.await.unwrap();
            assert_eq!(*ws.docs.active.peek(), Some(Files::node("saved.md").id));
        })
    });
}
#[test]
fn closing_a_background_tab_cancels_saved_focus() {
    scenario(|ws, files| {
        runtime().block_on(async {
            let restore = ws.restore_documents(
                Files::folder(),
                vec!["next.md".into(), "saved.md".into()],
                Some("saved.md".into()),
            );
            let mut restore = std::pin::pin!(restore);
            assert!(restore.as_mut().now_or_never().is_none());
            ws.close_node(Files::node("next.md").id);
            files.release.notify_one();
            restore.await;
            assert!(ws.document(Files::node("next.md").id).is_none());
            assert!(ws.docs.active.peek().is_none());
        })
    });
}

#[test]
fn guarded_background_load_preserves_foreground_and_unsaved_target_and_discards_stale_fetch() {
    scenario(|ws, files| {
        runtime().block_on(async {
            ws.open_node(Files::node("user.md")).await.unwrap();
            let current = std::rc::Rc::new(std::cell::Cell::new(true));
            let check = current.clone();
            let mut load =
                std::pin::pin!(ws.load_node_guarded(Files::node("saved.md"), move || check.get()));
            assert!(load.as_mut().now_or_never().is_none());
            current.set(false);
            files.release.notify_one();
            assert!(!load.await.unwrap());
            assert!(ws.document(Files::node("saved.md").id).is_none());
            files.pause.store(false, Ordering::SeqCst);
            assert!(ws
                .load_node_guarded(Files::node("saved.md"), || true)
                .await
                .unwrap());
            assert_eq!(*ws.docs.active.peek(), Some(Files::node("user.md").id));
            let mut saved = ws.document(Files::node("saved.md").id).unwrap();
            saved.write().text = "# Unsaved target".into();
            assert!(ws
                .load_node_guarded(Files::node("saved.md"), || true)
                .await
                .unwrap());
            assert_eq!(saved.peek().text, "# Unsaved target");
            assert_eq!(*ws.docs.active.peek(), Some(Files::node("user.md").id));
        });
    });
}
