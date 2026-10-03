//! Milestone 16: the workspace follows changes made on disk. A scripted
//! folder source reports changed paths through `changes_since`; the
//! workspace resolves them, refreshes the index for each, bumps `fs_epoch`
//! and `graph_epoch`, lists the folder under `watched` — and a database
//! source, which is not watched, is refreshed by hand instead. Runs a real
//! `VirtualDom` so signals and `spawn_forever` work.

use dioxus::prelude::*;
use moonkale_core::{
    Applied, Capabilities, Changes, Node, NodeId, NodeKind, Query, QueryResult, Source,
    SourceDescriptor, SourceError, SourceFamily, SourceId, Transaction, Version,
};
use moonkale_ext_api::{OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use std::sync::{Arc, Mutex};

fn descriptor(id: &str, family: SourceFamily) -> SourceDescriptor {
    let id = SourceId::new(id);
    SourceDescriptor {
        root: NodeId::derive(&id, ""),
        id,
        display_name: "x".into(),
        family,
        capabilities: Capabilities::default(),
    }
}

/// A folder whose "disk" is a list of existing paths; its changes come from
/// the test through a channel.
struct Disk {
    d: SourceDescriptor,
    files: Mutex<Vec<String>>,
    feed: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Changes>>,
}

#[moonkale_core::async_trait]
impl Source for Disk {
    fn id(&self) -> SourceId {
        self.d.id.clone()
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.d.clone()
    }
    async fn query(&self, q: Query) -> Result<QueryResult, SourceError> {
        match q {
            Query::Text { dialect, text } if dialect == "path" => {
                if !self.files.lock().unwrap().contains(&text) {
                    return Err(SourceError::NotFound);
                }
                Ok(QueryResult::single(Node {
                    id: NodeId::derive(&self.d.id, &text),
                    source: self.d.id.clone(),
                    kind: NodeKind::File,
                    label: text.clone(),
                    native_key: text,
                    content: None,
                    version: Version::default(),
                }))
            }
            _ => Ok(QueryResult::default()),
        }
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::NotFound)
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("disk".into()))
    }
    async fn changes_since(&self, since: u64) -> Result<Option<Changes>, SourceError> {
        if since == 0 {
            return Ok(Some(Changes {
                seq: 1,
                ..Changes::default()
            }));
        }
        match self.feed.lock().await.recv().await {
            Some(c) => Ok(Some(c)),
            None => futures_pending().await,
        }
    }
}

async fn futures_pending<T>() -> T {
    std::future::pending().await
}

/// The index: remembers which nodes it was asked to refresh.
struct Index {
    d: SourceDescriptor,
    refreshed: Arc<Mutex<Vec<NodeId>>>,
}

#[moonkale_core::async_trait]
impl Source for Index {
    fn id(&self) -> SourceId {
        self.d.id.clone()
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.d.clone()
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        Ok(QueryResult::default())
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::NotFound)
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("index".into()))
    }
    async fn refresh(&self, node: NodeId) -> Result<(), SourceError> {
        self.refreshed.lock().unwrap().push(node);
        Ok(())
    }
}

/// A database: not watched (the default `changes_since`).
struct Db(SourceDescriptor);

#[moonkale_core::async_trait]
impl Source for Db {
    fn id(&self) -> SourceId {
        self.0.id.clone()
    }
    fn descriptor(&self) -> SourceDescriptor {
        self.0.clone()
    }
    async fn query(&self, _: Query) -> Result<QueryResult, SourceError> {
        Ok(QueryResult::default())
    }
    async fn fetch_text(&self, _: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::NotFound)
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("db".into()))
    }
}

fn open_folder(_: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}
fn attach(_: SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

fn config() -> WorkspaceConfig {
    WorkspaceConfig {
        folders: moonkale_ext_api::FolderAccess {
            open: open_folder,
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
    }
}

type Setup = (
    Workspace,
    Arc<Disk>,
    Arc<Mutex<Vec<NodeId>>>,
    tokio::sync::mpsc::UnboundedSender<Changes>,
);

thread_local! {
    static SETUP: std::cell::RefCell<Option<Setup>> = const { std::cell::RefCell::new(None) };
}

#[component]
fn App() -> Element {
    let mut ws = use_context_provider(|| Workspace::new(config()));
    use_hook(move || {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let disk = Arc::new(Disk {
            d: descriptor("folder:/notes", SourceFamily::Folder),
            files: Mutex::new(vec!["a.md".into(), "gone.md".into()]),
            feed: tokio::sync::Mutex::new(rx),
        });
        let refreshed = Arc::new(Mutex::new(Vec::new()));
        ws.add_source(disk.clone());
        ws.add_source(Arc::new(Index {
            d: descriptor("index:folder:/notes", SourceFamily::Index),
            refreshed: refreshed.clone(),
        }));
        ws.add_source(Arc::new(Db(descriptor(
            "sqlite:/data.db",
            SourceFamily::Sql,
        ))));
        ws.follow_sources();
        SETUP.with(|s| *s.borrow_mut() = Some((ws, disk, refreshed, tx)));
    });
    rsx! { div { "{ws.sources.fs_epoch}" } }
}

async fn settle(dom: &mut VirtualDom) {
    for _ in 0..20 {
        tokio::select! {
            _ = dom.wait_for_work() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
        }
        dom.render_immediate(&mut dioxus_core::NoOpMutations);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn changes_on_disk_reach_the_index_and_the_explorer() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    let (ws, disk, refreshed, tx) = SETUP.with(|s| s.borrow_mut().take()).unwrap();
    let folder = SourceId::new("folder:/notes");
    let db = SourceId::new("sqlite:/data.db");
    let index = SourceId::new("index:folder:/notes");

    let watched = ws.sources.watched.peek().clone();
    assert!(watched.contains(&folder), "the folder answered: watched");
    assert!(!watched.contains(&db), "the database is not watched");
    assert!(
        !watched.contains(&index),
        "the index is derived, not watched"
    );

    // Two paths change on disk: one edited, one deleted.
    let epoch = *ws.sources.fs_epoch.peek();
    let graph_epoch = *ws.sources.graph_epoch.peek();
    disk.files.lock().unwrap().retain(|f| f != "gone.md");
    tx.send(Changes {
        seq: 2,
        paths: vec!["a.md".into(), "gone.md".into()],
        reset: false,
    })
    .unwrap();
    settle(&mut dom).await;
    let got = refreshed.lock().unwrap().clone();
    assert_eq!(
        got,
        [
            NodeId::derive(&folder, "a.md"),
            NodeId::derive(&folder, "gone.md")
        ],
        "the index re-reads both, the deleted one by its derived id"
    );
    assert!(*ws.sources.fs_epoch.peek() > epoch, "the Explorer reloads");
    assert!(
        *ws.sources.graph_epoch.peek() > graph_epoch,
        "the graph reloads"
    );

    // A burst: refresh from the root.
    refreshed.lock().unwrap().clear();
    tx.send(Changes {
        seq: 900,
        paths: vec![],
        reset: true,
    })
    .unwrap();
    settle(&mut dom).await;
    assert_eq!(
        refreshed.lock().unwrap().clone(),
        [NodeId::derive(&folder, "")]
    );

    // The database has no watcher: its ↻ button re-reads it and reloads.
    let epoch = *ws.sources.fs_epoch.peek();
    dom.in_scope(ScopeId::ROOT, || {
        let db = db.clone();
        spawn(async move { ws.refresh_source(&db).await });
    });
    settle(&mut dom).await;
    assert!(*ws.sources.fs_epoch.peek() > epoch);

    // Closing the folder ends its loop: it leaves `watched`.
    let mut ws2 = ws;
    ws2.sources
        .open
        .with_mut(|v| v.retain(|s| s.descriptor.id != folder));
    tx.send(Changes {
        seq: 901,
        paths: vec!["a.md".into()],
        reset: false,
    })
    .unwrap();
    settle(&mut dom).await;
    assert!(!ws.sources.watched.peek().contains(&folder));
}
