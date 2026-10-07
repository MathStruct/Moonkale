//! Desktop entrypoint: an undecorated window whose title bar (menus + window
//! controls) is drawn by `moonkale_shell::Frame`, VS Code style. Everything that touches
//! `dioxus::desktop` lives in this file; `ui` only receives callbacks.
//!
//! Multiple windows: *View → New Window* spawns another `App` in the same
//! process. Windows share sources through a process-wide registry and talk
//! over an in-process session bus (documents can be dragged between them).

use dioxus::prelude::*;
use futures_channel::mpsc;
use futures_util::StreamExt;
use moonkale_core::{Source, SourceDescriptor, SourceError};
use moonkale_shell::{
    AttachFuture, Frame, OpenFolderFuture, PickFolderFuture, SessionBus, SessionMessage, Shell,
    ShellConfig, WindowControls, WindowId, WorkspaceConfig,
};
use moonkale_sources::SourceRegistry;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // Milestone 11: server functions always go to a loopback relay of our
    // own (dioxus's server URL can be set only once, before launch — P-098);
    // the relay pipes to the remote that is active, if any.
    match moonkale_server::relay::install() {
        Ok(url) => dioxus::fullstack::set_server_url(url.leak()),
        Err(e) => eprintln!("moonkale: client relay not started ({e}); remote folders are off"),
    }
    // `MOONKALE_REMOTE=http://host:port` (+ `MOONKALE_TOKEN`) makes this
    // desktop a client of that Moonkale server from the start.
    if let Ok(url) = std::env::var("MOONKALE_REMOTE") {
        let token = std::env::var("MOONKALE_TOKEN").ok();
        moonkale_server::client::connect(&url, token.as_deref(), &url);
        eprintln!("moonkale: remote mode — sources, terminal, LSP and git on {url}");
    }
    #[cfg(all(feature = "desktop", target_os = "linux"))]
    mute_webkit_exit_dumps();
    #[cfg(feature = "desktop")]
    dioxus::LaunchBuilder::new()
        .with_cfg(window_config())
        .launch(App);
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// P-061: with the proprietary NVIDIA driver, WebKitGTK's web process
/// segfaults inside `libnvidia-eglcore` while tearing down a live WebGL
/// context — i.e. every time this app exits or `dx serve` kills it for a
/// rebuild. The process is exiting anyway; the only effect is a core dump
/// and a crash popup per run. Setting the soft core limit to 0 here is
/// inherited by the WebKit child processes and stops those dumps. Set
/// `MOONKALE_COREDUMPS=1` to keep them (for debugging real crashes).
#[cfg(all(feature = "desktop", target_os = "linux"))]
fn mute_webkit_exit_dumps() {
    if std::env::var_os("MOONKALE_COREDUMPS").is_some()
        || !std::path::Path::new("/proc/driver/nvidia/version").exists()
    {
        return;
    }
    // SAFETY: plain libc calls on a valid, initialised rlimit struct.
    unsafe {
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_CORE, &mut lim) == 0 {
            lim.rlim_cur = 0;
            let _ = libc::setrlimit(libc::RLIMIT_CORE, &lim);
        }
    }
    eprintln!("moonkale: NVIDIA driver detected; core dumps disabled for this process tree (P-061, MOONKALE_COREDUMPS=1 to keep)");
}

/// Sources are shared by every window of the process, so a folder opened in
/// one window is the same `FolderSource` (same ids, same version checks) in
/// another.
fn registry() -> &'static SourceRegistry {
    static REGISTRY: OnceLock<SourceRegistry> = OnceLock::new();
    REGISTRY.get_or_init(SourceRegistry::new)
}

/// Native build: open the folder in-process with `FolderSource`, index it,
/// and register both process-wide. A `.sqlite`/`.db` file or a
/// `.lbug`/`.kuzu` database opens as that database instead.
/// Milestone 11: when the desktop is connected to a Moonkale server
/// (`moonkale_server::client::active()`), sources, terminal, LSP, git, Typst and wasm
/// extensions go to that server; otherwise everything is local. The LLM
/// provider is always local (keys stay on this machine).
fn open_folder(path: String, options: moonkale_shell::OpenOptions) -> OpenFolderFuture {
    if moonkale_server::client::active().is_some() {
        return moonkale_server::client::open_folder(path, options);
    }
    open_local(path, options)
}

fn open_local(path: String, options: moonkale_shell::OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path
        };
        if let Some(db) = open_database(&path).await? {
            if let Some(existing) = registry().get(&db.id()) {
                return Ok(vec![existing]);
            }
            registry().insert(db.clone());
            return Ok(vec![db]);
        }
        let folder = moonkale_project_fs::FolderSource::open(&path).map_err(SourceError::from)?;
        let id = folder.id();
        if let Some(existing) = registry().get(&id) {
            let index_id = moonkale_core::SourceId::new(format!("index:{id}"));
            let mut v = vec![existing];
            v.extend(registry().get(&index_id));
            return Ok(v);
        }
        let folder: Arc<dyn Source> = Arc::new(folder);
        registry().insert(folder.clone());
        tracing::info!("open_local: folder registered, building index");
        // Embeddings (if configured) fill in the background; search is
        // BM25-only until they arrive.
        let embedder = options.embed.as_ref().map(provider_for);
        let index =
            Arc::new(moonkale_index::IndexSource::build_with(folder.clone(), embedder).await?);
        tracing::info!("open_local: index built");
        registry().insert(index.clone());
        let bg = index.clone();
        spawn(async move {
            match bg.embed_pending().await {
                Ok(n) if n > 0 => eprintln!("moonkale: embedded {n} chunks"),
                Ok(_) => {}
                Err(e) => eprintln!("moonkale: embedding failed: {e}"),
            }
        });
        let index: Arc<dyn Source> = index;
        Ok(vec![folder, index])
    })
}

/// Database files/directories (the app's source openers); `None` means
/// "treat as a folder".
async fn open_database(path: &str) -> Result<Option<Arc<dyn Source>>, SourceError> {
    let p = std::path::PathBuf::from(path);
    let is_dir = p.is_dir();
    openers().open(p, is_dir).await
}

/// Another window opened it: take the shared instance from the registry.
fn attach_source(descriptor: SourceDescriptor) -> AttachFuture {
    if moonkale_server::client::active().is_some() {
        return moonkale_server::client::attach_source(descriptor);
    }
    attach_local(descriptor)
}

fn attach_local(descriptor: SourceDescriptor) -> AttachFuture {
    Box::pin(async move {
        if let Some(s) = registry().get(&descriptor.id) {
            return Ok(s);
        }
        // Not in the registry (shouldn't happen in-process): re-open by path.
        let id = descriptor.id.as_str();
        let path = id
            .strip_prefix("folder:")
            .or_else(|| id.strip_prefix("sqlite:"))
            .or_else(|| id.strip_prefix("ladybug:"))
            .or_else(|| id.strip_prefix("turso:"))
            .or_else(|| id.strip_prefix("redb:"))
            .or_else(|| id.strip_prefix("rocksdb:"))
            .or_else(|| id.strip_prefix("helix:"))
            .unwrap_or(".")
            .to_string();
        let opened = open_local(path, moonkale_shell::OpenOptions::default()).await?;
        opened
            .into_iter()
            .find(|s| s.id() == descriptor.id)
            .ok_or(SourceError::NotFound)
    })
}

// ---- Remote folders over SSH (Milestone 11) ----

/// `File → Open Remote Folder…`: start the ssh session; the workspace gets
/// the `ssh` terminal and a handle to close it.
fn open_remote(
    host: String,
    path: String,
    sink: moonkale_shell::remote::PhaseSink,
) -> Result<moonkale_shell::remote::Opened, String> {
    use moonkale_remote::Phase;
    let target = moonkale_remote::SshTarget::parse(&host, &path)?;
    let binary = moonkale_remote::server_binary();
    if binary.is_none() {
        tracing::warn!("remote: no moonkale-server binary to upload (MOONKALE_SERVER_BINARY)");
    }
    let (session, tee) = moonkale_remote::SshSession::open(target, binary, move |p| {
        sink(match p {
            Phase::Connecting => moonkale_shell::remote::RemotePhase::Connecting,
            Phase::Prompt(l) => moonkale_shell::remote::RemotePhase::Prompt(l),
            Phase::Uploading => moonkale_shell::remote::RemotePhase::Uploading,
            Phase::Starting => moonkale_shell::remote::RemotePhase::Starting,
            Phase::Ready { .. } => moonkale_shell::remote::RemotePhase::Ready,
            Phase::Failed(e) => moonkale_shell::remote::RemotePhase::Failed(e),
            Phase::Closed => moonkale_shell::remote::RemotePhase::Closed,
        })
    })?;
    Ok((Box::new(tee), Arc::new(RemoteHandle(session))))
}

struct RemoteHandle(moonkale_remote::SshSession);
impl moonkale_shell::remote::RemoteSession for RemoteHandle {
    fn close(&self) {
        self.0.close();
    }
}

/// `Host` aliases of `~/.ssh/config` (no wildcards), for the dialog.
fn ssh_hosts() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(std::path::Path::new(&home).join(".ssh/config")) else {
        return Vec::new();
    };
    ssh_hosts_in(&text)
}

fn ssh_hosts_in(text: &str) -> Vec<String> {
    let mut hosts: Vec<String> = text
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l
                .strip_prefix("Host ")
                .or_else(|| l.strip_prefix("Host\t"))?;
            Some(
                rest.split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .filter(|h| !h.contains('*') && !h.contains('?') && !h.starts_with('!'))
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// *Connect to Server…* (Milestone 12): URL + token through the relay.
fn server_client() -> moonkale_shell::ServerClient {
    moonkale_shell::ServerClient {
        connect: |url, token| {
            moonkale_server::client::connect(&url, token.as_deref(), &url);
            Ok(())
        },
        disconnect: moonkale_server::client::disconnect,
        active: || moonkale_server::client::active().map(|r| r.label),
    }
}

/// `MOONKALE_SSH='[VAR=v …] [ssh options …] host:/path'` (or `--ssh …`):
/// open that remote folder when the window starts — `--ssh "SSH_AUTH_SOCK=0
/// -p 443 daniel@192.168.178.62:/home/daniel/Code"`. The last word is
/// `host:path`; everything before it is what `ssh` gets.
fn ssh_at_start() -> Option<(String, String)> {
    let spec = std::env::var("MOONKALE_SSH").ok().or_else(|| {
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            if a == "--ssh" {
                return args.next();
            }
            if let Some(v) = a.strip_prefix("--ssh=") {
                return Some(v.to_string());
            }
        }
        None
    })?;
    let spec = spec.trim();
    let (head, last) = spec.rsplit_once(char::is_whitespace).unwrap_or(("", spec));
    let (host, path) = last.split_once(':')?;
    let host = if head.is_empty() {
        host.to_string()
    } else {
        format!("{head} {host}")
    };
    Some((host, path.to_string()))
}

/// Local terminal: the user's shell in a PTY — or the server's (Milestone 11).
fn spawn_terminal(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> moonkale_terminal::SpawnTerminalFuture {
    if moonkale_server::client::active().is_some() {
        return moonkale_server::client::spawn_terminal(cwd, cols, rows);
    }
    Box::pin(async move {
        moonkale_terminal_pty::PtyBackend::spawn(cwd.as_deref(), None, cols, rows)
            .map(|p| Box::new(p) as Box<dyn moonkale_terminal::TerminalBackend>)
    })
}

/// A program under a PTY as a terminal tab (Milestone 15: `claude auth login`).
fn spawn_program(
    program: String,
    args: Vec<String>,
    cols: u16,
    rows: u16,
) -> moonkale_terminal::SpawnTerminalFuture {
    Box::pin(async move {
        moonkale_terminal_pty::PtyBackend::spawn_args(None, Some(&program), &args, cols, rows)
            .map(|p| Box::new(p) as Box<dyn moonkale_terminal::TerminalBackend>)
    })
}

/// Typst compiles in-process (embedded fonts) — or on the server.
fn compile_typst(
    root: String,
    main_rel: String,
    text: String,
) -> moonkale_shell::CompileTypstFuture {
    if moonkale_server::client::active().is_some() {
        return moonkale_editor_markdown::remote_typst(root, main_rel, text);
    }
    Box::pin(async move {
        moonkale_typst::compile_to_svg(std::path::Path::new(&root), &main_rel, text).map_err(|d| {
            d.into_iter()
                .map(|d| match d.hint {
                    Some(h) => format!("{} (hint: {h})", d.message),
                    None => d.message,
                })
                .collect()
        })
    })
}

/// Language servers run locally over stdio — or on the server.
fn spawn_lsp(language: String, root: String) -> moonkale_lsp::LspTransportFuture {
    if moonkale_server::client::active().is_some() {
        return moonkale_server::client::spawn_lsp(language, root);
    }
    Box::pin(async move {
        let spec = moonkale_lsp_local::discover::find(&language).ok_or_else(|| {
            match moonkale_lsp_local::discover::install_hint(&language) {
                Some(h) => format!("no language server for {language} (install: {h})"),
                None => format!("no language server known for {language}"),
            }
        })?;
        let args: Vec<&str> = spec.args.iter().map(String::as_str).collect();
        moonkale_lsp_local::StdioTransport::spawn(&spec.program, &args, &root)
            .map(|t| Box::new(t) as Box<dyn moonkale_lsp::LspTransport>)
    })
}

/// The LLM provider from the environment (`MOONKALE_LLM`, keys), built
/// once; HTTP providers run in-process on desktop.
fn llm_provider(settings: moonkale_llm::LlmSettings) -> moonkale_shell::LlmProviderFuture {
    let needs_key = !matches!(
        settings.provider.as_str(),
        "mock" | "ollama" | "claude-code"
    );
    if needs_key && !moonkale_llm::secrets::available(&settings.secret) {
        let name = settings.secret.clone();
        return Box::pin(async move {
            Err(format!(
                "no secret named {name:?}: set MOONKALE_SECRET_{} or add it in Settings",
                name.to_ascii_uppercase().replace('-', "_")
            ))
        });
    }
    let p = provider_for(&settings);
    Box::pin(async move { Ok(p) })
}

/// Providers built from settings, cached by settings (a chat and the index
/// share one client).
fn provider_for(settings: &moonkale_llm::LlmSettings) -> Arc<dyn moonkale_llm::Provider> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: OnceLock<
        Mutex<HashMap<moonkale_llm::LlmSettings, Arc<dyn moonkale_llm::Provider>>>,
    > = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap();
    if let Some(p) = map.get(settings) {
        return p.clone();
    }
    let key = moonkale_llm::secrets::resolve(&settings.secret);
    let cfg = moonkale_llm::Config::from_settings(settings, key);
    eprintln!("moonkale: llm provider {}", cfg.label());
    let p: Arc<dyn moonkale_llm::Provider> = Arc::from(moonkale_llm::config::build(&cfg));
    map.insert(settings.clone(), p.clone());
    p
}

/// User settings: `<config dir>/moonkale/settings.json`.
fn settings_path() -> Option<std::path::PathBuf> {
    moonkale_llm::secrets::config_dir().map(|d| d.join("settings.json"))
}

fn load_settings() -> moonkale_shell::SettingsFuture<moonkale_shell::SettingsFile> {
    Box::pin(async move {
        let Some(path) = settings_path() else {
            return Ok(moonkale_shell::SettingsFile::new());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => moonkale_shell::SettingsFile::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(moonkale_shell::SettingsFile::new())
            }
            Err(e) => Err(e.to_string()),
        }
    })
}

fn store_secret(name: String, value: String) -> moonkale_shell::SettingsFuture<()> {
    Box::pin(async move { moonkale_llm::secrets::store(&name, &value) })
}

fn save_settings(file: moonkale_shell::SettingsFile) -> moonkale_shell::SettingsFuture<()> {
    Box::pin(async move {
        let path = settings_path().ok_or("no config directory on this platform")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, file.to_json()).map_err(|e| e.to_string())
    })
}

/// wasm extensions run in-process (wasmtime) over the process registry.
fn git_local(
    root: String,
    req: moonkale_ext_git::GitRequest,
) -> moonkale_shell::SettingsFuture<moonkale_ext_git::GitResponse> {
    if moonkale_server::client::active().is_some() {
        return moonkale_ext_git::remote(root, req);
    }
    Box::pin(async move { moonkale_ext_git::cli::run(std::path::Path::new(&root), req).await })
}

mod presence;

mod wasm_ext {
    use super::registry;
    use moonkale_core::{Source, SourceDescriptor, SourceId};
    use moonkale_ext_host::{discover, Host, Runtime, WasmManifest};
    use std::sync::{Arc, Mutex, OnceLock};

    struct RegistryHost;
    impl Host for RegistryHost {
        fn list_sources(&self) -> Vec<SourceDescriptor> {
            registry().descriptors()
        }
        fn source(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
            registry().get(id)
        }
    }

    fn runtime() -> &'static Mutex<Runtime> {
        static RT: OnceLock<Mutex<Runtime>> = OnceLock::new();
        RT.get_or_init(|| Mutex::new(Runtime::new().expect("wasmtime engine")))
    }

    /// Server-side modules when connected (Milestone 11), local ones otherwise.
    pub fn list_any(folder: Option<String>) -> moonkale_shell::SettingsFuture<Vec<WasmManifest>> {
        if moonkale_server::client::active().is_some() {
            return moonkale_server::client::wasm_list(folder);
        }
        list(folder)
    }

    pub fn run_any(
        ext: String,
        command: String,
        args: serde_json::Value,
        granted: Vec<String>,
    ) -> moonkale_shell::SettingsFuture<String> {
        if moonkale_server::client::active().is_some() {
            return moonkale_server::client::wasm_run(ext, command, args, granted);
        }
        run(ext, command, args, granted)
    }

    pub fn list(folder: Option<String>) -> moonkale_shell::SettingsFuture<Vec<WasmManifest>> {
        Box::pin(async move {
            let config = moonkale_llm::secrets::config_dir();
            let folder = folder.map(std::path::PathBuf::from);
            let paths = discover(config.as_deref(), folder.as_deref());
            let mut rt = runtime().lock().unwrap();
            for p in &paths {
                if let Err(e) = rt.load(p) {
                    eprintln!("moonkale: extension {}: {e}", p.display());
                }
            }
            Ok(rt.extensions.iter().map(|e| e.manifest.clone()).collect())
        })
    }

    pub fn run(
        ext: String,
        command: String,
        args: serde_json::Value,
        granted: Vec<String>,
    ) -> moonkale_shell::SettingsFuture<String> {
        Box::pin(async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                let rt = runtime().lock().unwrap();
                rt.run(
                    &ext,
                    &command,
                    args,
                    granted,
                    Arc::new(RegistryHost),
                    handle,
                )
            })
            .await
            .map_err(|e| e.to_string())?
        })
    }
}

/// Native folder picker. rfd's xdg-portal backend talks to the desktop's
/// portal daemon and resolves to `None` when cancelled or unavailable.
fn pick_folder() -> PickFolderFuture {
    Box::pin(async move {
        rfd::AsyncFileDialog::new()
            .set_title("Open Folder")
            .pick_folder()
            .await
            .map(|h| h.path().to_string_lossy().into_owned())
    })
}

// ---- session bus: every window runs on the main thread, so a thread-local
// list of per-window channels is all the transport needed.

thread_local! {
    static PEERS: RefCell<Vec<(WindowId, mpsc::UnboundedSender<SessionMessage>)>> = const { RefCell::new(Vec::new()) };
}

struct InProcessBus {
    me: WindowId,
}

impl SessionBus for InProcessBus {
    fn send(&self, msg: SessionMessage) {
        PEERS.with(|peers| {
            // Deliver to every other window; drop peers whose window is gone.
            peers
                .borrow_mut()
                .retain(|(id, tx)| id == &self.me || tx.unbounded_send(msg.clone()).is_ok());
        });
    }
}

fn session(deliver: Callback<SessionMessage>) -> Rc<dyn SessionBus> {
    let me = WindowId::fresh();
    let (tx, mut rx) = mpsc::unbounded();
    // Sources are process-wide: hand the new window everything already open
    // without waiting for a peer to answer `Hello`.
    for descriptor in registry().descriptors() {
        let _ = tx.unbounded_send(SessionMessage::SourceOpened {
            from: WindowId("process-registry".into()),
            descriptor,
        });
    }
    PEERS.with(|peers| peers.borrow_mut().push((me.clone(), tx)));
    spawn(async move {
        while let Some(msg) = rx.next().await {
            deliver.call(msg);
        }
    });
    Rc::new(InProcessBus { me })
}

#[cfg(feature = "desktop")]
fn window_config() -> dioxus::desktop::Config {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    // Window/taskbar icon: raw RGBA (64×64) so no image decoder is needed;
    // regenerate with `magick assets/Moonkale64.png -depth 8 rgba:packages/desktop/assets/icon64.rgba`.
    let icon = dioxus::desktop::tao::window::Icon::from_rgba(
        include_bytes!("../assets/icon64.rgba").to_vec(),
        64,
        64,
    )
    .ok();
    let window = WindowBuilder::new()
        .with_title("Moonkale")
        .with_window_icon(icon)
        .with_inner_size(LogicalSize::new(1400.0, 900.0))
        // Small enough to try the phone-sized shell (< 700 px) on desktop.
        .with_min_inner_size(LogicalSize::new(360.0, 400.0))
        // No native decorations: the title bar is ours (see moonkale_shell::TitleBar).
        .with_decorations(false);
    Config::new()
        .with_window(window)
        // Drop dioxus-desktop's default native menu bar; ours replaces it,
        // including the "Toggle Developer Tools" entry.
        .with_menu(None)
        .with_disable_context_menu(true)
        // The web server's policy (moonkale_server::auth::CSP, #10) as far as a
        // meta tag can carry it (`frame-ancestors` only works as a header).
        .with_custom_head(
            r#"<meta http-equiv="Content-Security-Policy" content="object-src 'none'; base-uri 'none'; form-action 'self'">"#
                .to_string(),
        )
}

/// View → New Window: a second window running the same app; it joins the
/// session bus and receives the open sources from its peers.
#[cfg(feature = "desktop")]
fn new_window() {
    let dom = VirtualDom::new(App);
    dioxus::desktop::window().new_window(dom, window_config());
}

#[cfg(feature = "desktop")]
fn window_controls() -> WindowControls {
    use dioxus::desktop::{tao::window::ResizeDirection, window};
    use moonkale_shell::ResizeEdge;
    WindowControls {
        minimize: Callback::new(|_| window().window.set_minimized(true)),
        toggle_maximize: Callback::new(|_| window().toggle_maximized()),
        close: Callback::new(|_| window().close()),
        drag: Callback::new(|_| window().drag()),
        resize: Callback::new(|edge: ResizeEdge| {
            let dir = match edge {
                ResizeEdge::North => ResizeDirection::North,
                ResizeEdge::South => ResizeDirection::South,
                ResizeEdge::East => ResizeDirection::East,
                ResizeEdge::West => ResizeDirection::West,
                ResizeEdge::NorthEast => ResizeDirection::NorthEast,
                ResizeEdge::NorthWest => ResizeDirection::NorthWest,
                ResizeEdge::SouthEast => ResizeDirection::SouthEast,
                ResizeEdge::SouthWest => ResizeDirection::SouthWest,
            };
            let _ = window().window.drag_resize_window(dir);
        }),
        devtools: if cfg!(debug_assertions) {
            Some(Callback::new(|_| window().devtool()))
        } else {
            None
        },
    }
}

#[component]
fn App() -> Element {
    #[cfg(feature = "desktop")]
    let (controls, open_window) = (Some(window_controls()), Some(new_window as fn()));
    #[cfg(not(feature = "desktop"))]
    let (controls, open_window): (Option<WindowControls>, Option<fn()>) = (None, None);

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: moonkale_distribution::default_extensions,
                workspace: workspace_config(),
                session,
                new_window: open_window,
            },
            controls,
            Shell {}
        }
    }
}

/// The source openers of this app: every driver crate's (Milestone 18
/// phase 2). Moves to the `distribution` crate in phase 4.
fn openers() -> &'static moonkale_core::Openers {
    static OPENERS: std::sync::OnceLock<moonkale_core::Openers> = std::sync::OnceLock::new();
    OPENERS.get_or_init(|| {
        moonkale_core::Openers::new(
            [
                moonkale_sources_sql::openers(),
                moonkale_sources_kv::openers(),
                moonkale_sources_graph::openers(),
            ]
            .concat(),
        )
    })
}

/// This machine's state store (ADR-0014): `<config dir>/state.sqlite`.
fn state_access() -> Option<moonkale_shell::StateAccess> {
    let dir = moonkale_llm::secrets::config_dir()?;
    let _ = std::fs::create_dir_all(&dir);
    match moonkale_state_stores::SqliteStore::open_with(
        &dir.join("state.sqlite"),
        moonkale_state::Durability::Relaxed,
    ) {
        Ok(store) => Some(moonkale_shell::local_state(std::sync::Arc::new(store))),
        Err(e) => {
            tracing::warn!("state: no store ({e}); layouts stay in the folder");
            None
        }
    }
}

/// What this platform gives the workspace (Milestone 18 phase 3c: grouped by
/// what each part needs from the platform).
/// Services the extensions define (Milestone 18 phase 4.2).
static GIT: moonkale_ext_git::GitRunner = moonkale_ext_git::GitRunner(git_local);
/// The theme files (spec 030).
static THEMES: moonkale_shell::theme::ThemeFiles =
    moonkale_shell::theme::ThemeFiles(moonkale_server::themes::local_themes);
static SERVICES: [&(dyn std::any::Any + Sync); 2] = [&GIT, &THEMES];

fn workspace_config() -> WorkspaceConfig {
    let state = state_access();
    WorkspaceConfig {
        folders: moonkale_shell::FolderAccess {
            open: open_folder,
            pick: Some(pick_folder),
            attach: attach_source,
            reopen_last: true,
            openers: openers(),
        },
        processes: moonkale_shell::Processes {
            terminal: Some(spawn_terminal),
            lsp: Some(spawn_lsp),
            program: Some(spawn_program),
        },
        persistence: moonkale_shell::Persistence {
            settings: Some(moonkale_shell::SettingsStore {
                load: load_settings,
                save: save_settings,
            }),
            secrets: Some(store_secret),
            state,
            // The folder's host keeps its entity log: this store for local
            // folders, the server's while connected (phase 5.10).
            host: state.map(|_| moonkale_server::client::host_state_routed()),
            // The user's settings too (phase 5.18): `settings.json` is
            // imported once, then only the store is written.
            user_settings_in_state: true,
        },
        network: moonkale_shell::Network {
            presence: Some(presence::join),
            remote: Some(moonkale_shell::remote::RemoteHosts {
                open: open_remote,
                hosts: ssh_hosts,
                at_start: ssh_at_start,
            }),
            agent_sessions: Some(moonkale_server::client::agent_sessions(
                moonkale_server::client::agent_available,
            )),
            server: Some(server_client()),
        },
        runtimes: moonkale_shell::Runtimes {
            typst: Some(compile_typst),
            llm: Some(llm_provider),
            wasm: Some(moonkale_shell::WasmExtensions {
                list: wasm_ext::list_any,
                run: wasm_ext::run_any,
            }),
            ..Default::default()
        },
        services: &SERVICES,
    }
}

#[cfg(test)]
mod remote_tests {
    #[test]
    fn start_spec_keeps_options() {
        std::env::set_var(
            "MOONKALE_SSH",
            "SSH_AUTH_SOCK=0 -p 443 -v daniel@192.168.178.62:/home/daniel/Code",
        );
        assert_eq!(
            super::ssh_at_start(),
            Some((
                "SSH_AUTH_SOCK=0 -p 443 -v daniel@192.168.178.62".to_string(),
                "/home/daniel/Code".to_string()
            ))
        );
        std::env::set_var("MOONKALE_SSH", "box:/srv");
        assert_eq!(super::ssh_at_start(), Some(("box".into(), "/srv".into())));
        std::env::remove_var("MOONKALE_SSH");
    }

    #[test]
    fn ssh_config_hosts() {
        let cfg = "Host build-box\n  HostName 10.0.0.2\n\nHost *\n  ServerAliveInterval 30\nHost lab lab2 !lab-*\n\tUser me\nHost\tzed\n";
        assert_eq!(
            super::ssh_hosts_in(cfg),
            vec!["build-box", "lab", "lab2", "zed"]
        );
    }
}
