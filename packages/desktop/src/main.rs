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
    Frame, OpenFolderFuture, PickFolderFuture, SessionBus, SessionMessage, Shell, ShellConfig,
    WindowControls, WindowId, WorkspaceConfig,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    #[cfg(feature = "desktop")]
    dioxus::LaunchBuilder::new()
        .with_cfg(window_config())
        .launch(App);
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// Sources are shared by every window of the process, so a folder opened in
/// one window is the same `FolderSource` (same ids, same version checks) in
/// another.
fn registry() -> &'static SourceRegistry {
    static REGISTRY: OnceLock<SourceRegistry> = OnceLock::new();
    REGISTRY.get_or_init(SourceRegistry::new)
}

/// Native build: open the folder in-process with `FolderSource` and register
/// it process-wide.
fn open_local(path: String) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path
        };
        let source = moonkale_project_fs::FolderSource::open(&path).map_err(SourceError::from)?;
        let id = source.id();
        if let Some(existing) = registry().get(&id) {
            return Ok(existing);
        }
        let source: Arc<dyn Source> = Arc::new(source);
        registry().insert(source.clone());
        Ok(source)
    })
}

/// Another window opened it: take the shared instance from the registry.
fn attach_local(descriptor: SourceDescriptor) -> OpenFolderFuture {
    Box::pin(async move {
        match registry().get(&descriptor.id) {
            Some(s) => Ok(s),
            None => {
                let path = descriptor
                    .id
                    .as_str()
                    .strip_prefix("folder:")
                    .unwrap_or(".")
                    .to_string();
                open_local(path).await
            }
        }
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
                workspace: WorkspaceConfig { open_folder: open_local, pick_folder: Some(pick_folder), attach_source: attach_local },
                session,
                new_window: open_window,
            },
            controls,
            Shell {}
        }
    }
}
