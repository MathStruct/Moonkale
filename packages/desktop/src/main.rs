//! Desktop entrypoint: an undecorated window whose title bar (menus + window
//! controls) is drawn by `ui::Frame`, VS Code style. Everything that touches
//! `dioxus::desktop` lives in this file; `ui` only receives callbacks.
//!
//! Multiple windows: *View → New Window* spawns another `App` in the same
//! process. Windows share sources through a process-wide registry and talk
//! over an in-process session bus (documents can be dragged between them).

use dioxus::prelude::*;
use futures_channel::mpsc;
use futures_util::StreamExt;
use moonkale_core::{Source, SourceDescriptor, SourceError};
use moonkale_sources::SourceRegistry;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use ui::{
    AttachFuture, Frame, OpenFolderFuture, PickFolderFuture, SessionBus, SessionMessage, Shell,
    ShellConfig, WindowControls, WindowId, WorkspaceConfig,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
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
fn open_local(path: String, options: ui::OpenOptions) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path
        };
        if let Some(db) = open_database(&path)? {
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
        // Embeddings (if configured) fill in the background; search is
        // BM25-only until they arrive.
        let embedder = options.embed.as_ref().map(provider_for);
        let index =
            Arc::new(moonkale_index::IndexSource::build_with(folder.clone(), embedder).await?);
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

/// Database files/directories by name; `None` means "treat as a folder".
fn open_database(path: &str) -> Result<Option<Arc<dyn Source>>, SourceError> {
    let p = std::path::Path::new(path);
    if p.is_file() && moonkale_sources_sql::is_sqlite_path(path) {
        return Ok(Some(Arc::new(moonkale_sources_sql::SqliteSource::open(p)?)));
    }
    if moonkale_sources_graph::is_ladybug_path(path) {
        return Ok(Some(Arc::new(
            moonkale_sources_graph::ladybug::LadybugSource::open(p)?,
        )));
    }
    Ok(None)
}

/// Another window opened it: take the shared instance from the registry.
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
            .unwrap_or(".")
            .to_string();
        let opened = open_local(path, ui::OpenOptions::default()).await?;
        opened
            .into_iter()
            .find(|s| s.id() == descriptor.id)
            .ok_or(SourceError::NotFound)
    })
}

/// Local terminal: the user's shell in a PTY.
fn spawn_terminal(
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> moonkale_terminal::SpawnTerminalFuture {
    Box::pin(async move {
        moonkale_terminal_pty::PtyBackend::spawn(cwd.as_deref(), None, cols, rows)
            .map(|p| Box::new(p) as Box<dyn moonkale_terminal::TerminalBackend>)
    })
}

/// Typst compiles in-process (embedded fonts).
fn compile_typst(root: String, main_rel: String, text: String) -> ui::CompileTypstFuture {
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

/// Language servers run locally over stdio.
fn spawn_lsp(language: String, root: String) -> moonkale_lsp::LspTransportFuture {
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
fn llm_provider(settings: moonkale_llm::LlmSettings) -> ui::LlmProviderFuture {
    let needs_key = !matches!(settings.provider.as_str(), "mock" | "ollama");
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

fn load_settings() -> ui::SettingsFuture<ui::SettingsFile> {
    Box::pin(async move {
        let Some(path) = settings_path() else {
            return Ok(ui::SettingsFile::new());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => ui::SettingsFile::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ui::SettingsFile::new()),
            Err(e) => Err(e.to_string()),
        }
    })
}

fn store_secret(name: String, value: String) -> ui::SettingsFuture<()> {
    Box::pin(async move { moonkale_llm::secrets::store(&name, &value) })
}

fn save_settings(file: ui::SettingsFile) -> ui::SettingsFuture<()> {
    Box::pin(async move {
        let path = settings_path().ok_or("no config directory on this platform")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, file.to_json()).map_err(|e| e.to_string())
    })
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
    let window = WindowBuilder::new()
        .with_title("Moonkale")
        .with_inner_size(LogicalSize::new(1400.0, 900.0))
        .with_min_inner_size(LogicalSize::new(640.0, 400.0))
        // No native decorations: the title bar is ours (see ui::TitleBar).
        .with_decorations(false);
    Config::new()
        .with_window(window)
        // Drop dioxus-desktop's default native menu bar; ours replaces it,
        // including the "Toggle Developer Tools" entry.
        .with_menu(None)
        .with_disable_context_menu(true)
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
    use ui::ResizeEdge;
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
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_local, pick_folder: Some(pick_folder), attach_source: attach_local, spawn_terminal: Some(spawn_terminal), compile_typst: Some(compile_typst), spawn_lsp: Some(spawn_lsp), llm: Some(llm_provider), settings_store: Some(ui::SettingsStore { load: load_settings, save: save_settings }), secret_store: Some(store_secret) },
                session,
                new_window: open_window,
            },
            controls,
            Shell {}
        }
    }
}
