//! Listing hints and content preconditions may differ, and reads can fail.
use dioxus::prelude::*;
use moonkale_core::*;
use moonkale_ext_api::{FolderAccess, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

const CONTENT: Version = Version(22);
struct Files {
    text: Mutex<String>,
    unreadable: AtomicBool,
    writes: Mutex<Vec<Version>>,
}
impl Files {
    fn id() -> SourceId {
        SourceId::new("folder:listing-test")
    }
    fn node(path: &str) -> Node {
        Node {
            id: NodeId::derive(&Self::id(), path),
            source: Self::id(),
            kind: NodeKind::File,
            native_key: path.into(),
            label: path.into(),
            content: None,
            version: Version(11),
            props: Default::default(),
        }
    }
}
#[async_trait]
impl Source for Files {
    fn id(&self) -> SourceId {
        Self::id()
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: Self::id(),
            root: NodeId::derive(&Self::id(), ""),
            truth: Default::default(),
            display_name: "files".into(),
            family: SourceFamily::Folder,
            capabilities: Default::default(),
        }
    }
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Coarse { .. } => Err(moonkale_core::SourceError::Unsupported(
                "hierarchy coarsening".into(),
            )),
            Query::Text { text, .. } => Ok(QueryResult::single(Self::node(&text))),
            _ => Ok(QueryResult::default()),
        }
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        if self.unreadable.load(Ordering::SeqCst) {
            return Err(SourceError::Unsupported("unreadable".into()));
        }
        Ok((self.text.lock().unwrap().clone(), CONTENT))
    }
    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        let mut result = Applied::default();
        for op in tx.ops {
            if let Op::WriteText {
                node,
                expected,
                patch,
            } = op
            {
                self.writes.lock().unwrap().push(expected);
                if expected != CONTENT {
                    return Err(SourceError::Conflict {
                        expected,
                        actual: CONTENT,
                    });
                }
                let mut text = self.text.lock().unwrap();
                *text = patch.apply(&text)?;
                result.results.push(OpResult::Ok {
                    node,
                    version: CONTENT,
                    warnings: Vec::new(),
                });
            } else {
                return Err(SourceError::Unsupported("unexpected create".into()));
            }
        }
        Ok(result)
    }
}
fn open(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
thread_local! { static SETUP: std::cell::RefCell<Option<(Workspace, Arc<Files>)>> = const { std::cell::RefCell::new(None) }; }
fn app() -> Element {
    use_hook(|| {
        let mut ws = Workspace::new(WorkspaceConfig {
            folders: FolderAccess {
                open,
                pick: None,
                attach: |_| Box::pin(async { Err(SourceError::NotFound) }),
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
            text: Mutex::new("old".into()),
            unreadable: AtomicBool::new(false),
            writes: Mutex::new(Vec::new()),
        });
        ws.add_source(files.clone());
        ws.settings.folder.set(Some(Files::id()));
        SETUP.with(|s| *s.borrow_mut() = Some((ws, files)));
    });
    rsx! { div {} }
}
fn setup() -> (VirtualDom, Workspace, Arc<Files>) {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let (ws, files) = SETUP.with(|s| s.borrow_mut().take().unwrap());
    (dom, ws, files)
}
#[tokio::test(flavor = "current_thread")]
async fn state_file_writes_use_fetched_versions_and_refuse_failed_reads() {
    let (dom, ws, files) = setup();
    dom.in_scope(ScopeId::ROOT, || async {
        ws.write_text_at(&Files::id(), "state.json", "new")
            .await
            .unwrap();
        assert_eq!(*files.writes.lock().unwrap(), [CONTENT]);
        files.unreadable.store(true, Ordering::SeqCst);
        assert!(ws
            .write_text_at(&Files::id(), "state.json", "bad")
            .await
            .is_err());
        assert_eq!(files.writes.lock().unwrap().len(), 1);
        assert_eq!(*files.text.lock().unwrap(), "new");
    })
    .await;
}
#[tokio::test(flavor = "current_thread")]
async fn folder_settings_refuse_failed_reads_instead_of_using_listing_hints() {
    let (dom, ws, files) = setup();
    files.unreadable.store(true, Ordering::SeqCst);
    dom.in_scope(ScopeId::ROOT, || {
        ws.update_workspace_settings(|s| s.theme = Some("dark".into()))
    })
    .await;
    assert!(files.writes.lock().unwrap().is_empty());
    assert_eq!(*files.text.lock().unwrap(), "old");
}
#[tokio::test(flavor = "current_thread")]
async fn folder_settings_use_content_versions() {
    let (dom, ws, files) = setup();
    dom.in_scope(ScopeId::ROOT, || {
        ws.update_workspace_settings(|s| s.theme = Some("dark".into()))
    })
    .await;
    assert_eq!(*files.writes.lock().unwrap(), [CONTENT]);
    assert_ne!(*files.text.lock().unwrap(), "old");
}
#[tokio::test(flavor = "current_thread")]
async fn file_backed_history_uses_content_versions_and_refuses_failed_reads() {
    for unreadable in [false, true] {
        let (mut dom, mut ws, files) = setup();
        files.unreadable.store(unreadable, Ordering::SeqCst);
        dom.in_scope(ScopeId::ROOT, || {
            ws.record(EventKind::Remove {
                node: Files::node("gone").id,
            });
        });
        for _ in 0..10 {
            tokio::select! { _ = dom.wait_for_work() => {}, _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {} }
            dom.render_immediate(&mut dioxus_core::NoOpMutations);
        }
        if unreadable {
            assert!(files.writes.lock().unwrap().is_empty());
        } else {
            assert_eq!(*files.writes.lock().unwrap(), [CONTENT]);
            assert_ne!(*files.text.lock().unwrap(), "old");
        }
    }
}
