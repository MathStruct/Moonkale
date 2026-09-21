//! The workspace side of a remote folder (Milestone 11), without ssh: a
//! fake `OpenRemote` reports the phases, the workspace opens the folder
//! through `open_folder` when it is `Ready`, marks the sources as remote,
//! keeps them out of the recent list, and closing the folder ends the
//! session. Runs a real `VirtualDom` so signals and `spawn` work.

use dioxus::prelude::*;
use moonkale_core::{
    Applied, Capabilities, NodeId, Query, QueryResult, Source, SourceDescriptor, SourceError,
    SourceFamily, SourceId, Transaction, Version,
};
use moonkale_ext_api::remote::{PhaseSink, RemoteHosts, RemotePhase, RemoteSession};
use moonkale_ext_api::{Command, OpenFolderFuture, OpenOptions, Workspace, WorkspaceConfig};
use moonkale_terminal::{Output, TerminalBackend};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Mem(SourceDescriptor);
#[moonkale_core::async_trait]
impl Source for Mem {
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
        Err(SourceError::Unsupported("mem".into()))
    }
}

static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CLOSED: AtomicUsize = AtomicUsize::new(0);
static SINK: Mutex<Option<PhaseSink>> = Mutex::new(None);

fn open_folder(path: String, _: OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        OPENED.lock().unwrap().push(path.clone());
        let id = SourceId::new(format!("folder:{path}"));
        let d = SourceDescriptor {
            root: NodeId::derive(&id, ""),
            id,
            display_name: path,
            family: SourceFamily::Folder,
            capabilities: Capabilities {
                read: true,
                write: true,
                watch: false,
                text_query: None,
            },
        };
        Ok(vec![Arc::new(Mem(d)) as Arc<dyn Source>])
    })
}
fn attach(_: SourceDescriptor) -> moonkale_ext_api::AttachFuture {
    Box::pin(async { Err(SourceError::NotFound) })
}

struct Quiet;
impl TerminalBackend for Quiet {
    fn write(&self, _: &[u8]) {}
    fn resize(&self, _: u16, _: u16) {}
    fn take_output(&mut self) -> Option<Output> {
        None
    }
    fn title(&self) -> String {
        "ssh fake".into()
    }
}
struct Fake;
impl RemoteSession for Fake {
    fn close(&self) {
        CLOSED.fetch_add(1, Ordering::SeqCst);
    }
}
fn open_remote(
    host: String,
    path: String,
    sink: PhaseSink,
) -> Result<moonkale_ext_api::remote::Opened, String> {
    assert_eq!((host.as_str(), path.as_str()), ("box", "/srv/code"));
    *SINK.lock().unwrap() = Some(sink);
    Ok((Box::new(Quiet), Arc::new(Fake)))
}

static DONE: AtomicBool = AtomicBool::new(false);
thread_local! { static WS: std::cell::Cell<Option<Workspace>> = const { std::cell::Cell::new(None) }; }
fn ws() -> Workspace {
    WS.with(|w| w.get()).expect("workspace")
}

fn config() -> WorkspaceConfig {
    WorkspaceConfig {
        open_folder,
        pick_folder: None,
        attach_source: attach,
        spawn_terminal: None,
        compile_typst: None,
        spawn_lsp: None,
        llm: None,
        settings_store: None,
        secret_store: None,
        reopen_last_folder: false,
        wasm: None,
        git: None,
        presence: None,
        wasm_module_url: None,
        remote: Some(RemoteHosts {
            open: open_remote,
            hosts: || vec!["box".into()],
            at_start: || None,
        }),
        agent_sessions: None,
        server: None,
    }
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

#[component]
fn App() -> Element {
    let ws = use_context_provider(|| Workspace::new(config()));
    use_hook(move || {
        WS.with(|w| w.set(Some(ws)));
        assert_eq!(ws.remote_hosts(), vec!["box".to_string()]);
        ws.open_remote("box".into(), "/srv/code".into());
    });
    let remote = ws
        .remote
        .read()
        .as_ref()
        .map(|r| r.phase.label())
        .unwrap_or_default();
    let sources: Vec<String> = ws
        .sources
        .read()
        .iter()
        .map(|s| s.descriptor.id.to_string())
        .collect();
    let (_, cmd) = *ws.commands.read();
    rsx! { div { "{remote} | {sources:?} | {cmd:?}" } }
}

#[tokio::test(flavor = "current_thread")]
async fn phases_open_the_folder_and_closing_ends_the_session() {
    let mut dom = VirtualDom::new(App);
    dom.rebuild_in_place();
    fn phase(dom: &VirtualDom) -> (Option<RemotePhase>, usize, usize, usize, String) {
        let ws = dom.in_scope(ScopeId::ROOT, ws);
        let remote = ws.remote.peek().clone();
        let sources = ws.sources.peek().len();
        let adopt = ws.adopt_terminals.peek().len();
        let status = ws.status.peek().clone();
        (
            remote.as_ref().map(|r| r.phase.clone()),
            remote.as_ref().map(|r| r.sources.len()).unwrap_or(0),
            sources,
            adopt,
            status,
        )
    }
    let p = phase(&dom);
    assert_eq!(p.0, Some(RemotePhase::Connecting));
    assert_eq!(p.3, 1, "the ssh terminal is offered to the terminal panel");
    let send = |p: RemotePhase| (SINK.lock().unwrap().as_ref().unwrap())(p);
    send(RemotePhase::Prompt("box's password:".into()));
    settle(&mut dom).await;
    let p = phase(&dom);
    assert_eq!(p.0, Some(RemotePhase::Prompt("box's password:".into())));
    assert!(p.4.contains("answer in the terminal"), "{}", p.4);
    send(RemotePhase::Uploading);
    send(RemotePhase::Starting);
    send(RemotePhase::Ready);
    settle(&mut dom).await;
    let p = phase(&dom);
    assert_eq!(p.0, Some(RemotePhase::Ready));
    assert!(OPENED.lock().unwrap().iter().any(|p| p == "/srv/code"));
    assert_eq!(p.2, 1, "the remote folder is a source");
    assert_eq!(p.1, 1, "…and remembered as remote");
    let ws = dom.in_scope(ScopeId::ROOT, ws);
    let id = ws.sources.peek()[0].descriptor.id.clone();
    assert!(ws.is_remote_source(&id));
    assert!(
        ws.settings_user.peek().recent_folders.is_empty(),
        "remote paths are not recent folders"
    );
    // Closing the folder ends the session.
    let ws2 = ws;
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            ws2.close_source(&id).await.expect("close");
            DONE.store(true, Ordering::SeqCst);
        });
    });
    settle(&mut dom).await;
    assert!(DONE.load(Ordering::SeqCst));
    assert_eq!(CLOSED.load(Ordering::SeqCst), 1, "session closed once");
    let p = phase(&dom);
    assert_eq!(p.0, None);
    assert_eq!(p.2, 0);
    let _ = Command::CloseRemote;
}

// ---- Connect to Server… (Milestone 12) ----

static CONNECTED: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);
static DISCONNECTS: AtomicUsize = AtomicUsize::new(0);

fn server_config() -> WorkspaceConfig {
    let mut c = config();
    c.remote = None;
    c.server = Some(moonkale_ext_api::ServerClient {
        connect: |url, token| {
            *CONNECTED.lock().unwrap() = Some((url, token));
            Ok(())
        },
        disconnect: || {
            DISCONNECTS.fetch_add(1, Ordering::SeqCst);
        },
        active: || CONNECTED.lock().unwrap().as_ref().map(|(u, _)| u.clone()),
    });
    c
}

#[component]
fn ServerApp() -> Element {
    let ws = use_context_provider(|| Workspace::new(server_config()));
    use_hook(move || {
        WS.with(|w| w.set(Some(ws)));
        spawn(async move {
            ws.connect_server("http://box:8443/".into(), Some("tok".into()))
                .await;
        });
    });
    rsx! { div {} }
}

#[tokio::test(flavor = "current_thread")]
async fn connect_server_opens_the_root_and_disconnect_closes_it() {
    // Tests share the statics and run in parallel: check membership, not equality.
    let mut dom = VirtualDom::new(ServerApp);
    dom.rebuild_in_place();
    settle(&mut dom).await;
    assert_eq!(
        CONNECTED.lock().unwrap().clone(),
        Some(("http://box:8443".to_string(), Some("tok".to_string())))
    );
    // The server's root folder ("") was opened and recorded on the link.
    assert!(OPENED.lock().unwrap().iter().any(|p| p.is_empty()));
    let ws = dom.in_scope(ScopeId::ROOT, ws);
    let link = ws.server_link.peek().clone().expect("linked");
    assert_eq!(link.0, "http://box:8443");
    assert_eq!(link.1.len(), 1);
    assert!(
        ws.settings_user.peek().recent_folders.is_empty(),
        "server paths are not recents"
    );
    let mut ws2 = ws;
    dom.in_scope(ScopeId::ROOT, move || ws2.disconnect_server());
    assert_eq!(DISCONNECTS.load(Ordering::SeqCst), 1);
    assert!(ws.server_link.peek().is_none());
    assert_eq!(ws.sources.peek().len(), 0);
}
