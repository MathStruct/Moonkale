//! Desktop entrypoint: an undecorated window whose title bar (menus + window
//! controls) is drawn by `ui::Frame`, VS Code style. Everything that touches
//! `dioxus::desktop` lives in this file; `ui` only receives callbacks.

use dioxus::prelude::*;
use moonkale_core::{Source, SourceError};
use std::sync::Arc;
use ui::{
    Frame, OpenFolderFuture, PickFolderFuture, Shell, ShellConfig, WindowControls, WorkspaceConfig,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
        let window = WindowBuilder::new()
            .with_title("Moonkale")
            .with_inner_size(LogicalSize::new(1400.0, 900.0))
            .with_min_inner_size(LogicalSize::new(640.0, 400.0))
            // No native decorations: the title bar is ours (see ui::TitleBar).
            .with_decorations(false);
        dioxus::LaunchBuilder::new()
            .with_cfg(
                Config::new()
                    .with_window(window)
                    // Drop dioxus-desktop's default native menu bar; ours replaces it,
                    // including the "Toggle Developer Tools" entry.
                    .with_menu(None)
                    .with_disable_context_menu(true),
            )
            .launch(App);
    }
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

/// Native build: open the folder in-process with `FolderSource`.
fn open_local(path: String) -> OpenFolderFuture {
    Box::pin(async move {
        let path = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path
        };
        moonkale_project_fs::FolderSource::open(&path)
            .map(|s| Arc::new(s) as Arc<dyn Source>)
            .map_err(SourceError::from)
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
    let controls = Some(window_controls());
    #[cfg(not(feature = "desktop"))]
    let controls: Option<WindowControls> = None;

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Frame {
            config: ShellConfig {
                extensions: ui::default_extensions,
                workspace: WorkspaceConfig { open_folder: open_local, pick_folder: Some(pick_folder) },
            },
            controls,
            Shell {}
        }
    }
}
