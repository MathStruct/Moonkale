//! `Frame` — the outermost component: creates the workspace and extension
//! list once, provides both through context, renders the title bar, the
//! content, and (desktop only) invisible resize handles for the undecorated
//! window.

use crate::titlebar::TitleBar;
use dioxus::prelude::*;
use moonkale_ext_api::{Command, Extension, Workspace, WorkspaceConfig};
use std::rc::Rc;

/// Builds the extension list. A plain `fn` so it can be a prop.
pub type Extensions = fn() -> Vec<Box<dyn Extension>>;

/// What the platform hands the shell. Set once at startup; comparing
/// function pointers is meaningless, so two configs are always "equal" and
/// the frame never re-mounts because of them.
#[derive(Clone, Copy)]
pub struct ShellConfig {
    pub extensions: Extensions,
    pub workspace: WorkspaceConfig,
}

impl PartialEq for ShellConfig {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Which window edge a resize handle represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

/// Window operations the desktop crate implements with `dioxus-desktop`.
/// `ui` stays platform-agnostic by only ever calling these callbacks.
#[derive(Clone, Copy, PartialEq)]
pub struct WindowControls {
    pub minimize: Callback<()>,
    pub toggle_maximize: Callback<()>,
    pub close: Callback<()>,
    /// Start a window drag (called on mousedown in the title bar).
    pub drag: Callback<()>,
    /// Start an interactive resize from an edge.
    pub resize: Callback<ResizeEdge>,
    /// Open the webview inspector (debug builds); `None` hides the menu item.
    pub devtools: Option<Callback<()>>,
}

/// Extension list shared through context.
#[derive(Clone)]
pub struct Extensions_(pub Rc<Vec<Box<dyn Extension>>>);

#[component]
pub fn Frame(
    config: ShellConfig,
    #[props(default)] controls: Option<WindowControls>,
    children: Element,
) -> Element {
    let mut ws = use_context_provider(|| Workspace::new(config.workspace));
    use_context_provider(|| Extensions_(Rc::new((config.extensions)())));

    rsx! {
        div {
            class: "mk-frame",
            class: if controls.is_some() { "mk-frame-undecorated" },
            // Global keybindings. Ctrl+S is handled inside the editor panel
            // (it needs the panel's error state); the rest go through the bus.
            onkeydown: move |e| {
                let m = e.modifiers();
                if !(m.ctrl() || m.meta()) {
                    return;
                }
                let key = match e.key() { Key::Character(c) => c.to_ascii_lowercase(), _ => return };
                match (key.as_str(), m.shift()) {
                    ("w", false) => { e.prevent_default(); ws.dispatch(Command::CloseEditor); }
                    ("o", false) => { e.prevent_default(); ws.dispatch(Command::OpenFolder); }
                    _ => {}
                }
            },
            TitleBar { controls }
            div { class: "mk-frame-body", {children} }
            if let Some(c) = controls {
                ResizeHandles { controls: c }
            }
        }
    }
}

#[component]
fn ResizeHandles(controls: WindowControls) -> Element {
    let edges = [
        ("n", ResizeEdge::North),
        ("s", ResizeEdge::South),
        ("e", ResizeEdge::East),
        ("w", ResizeEdge::West),
        ("ne", ResizeEdge::NorthEast),
        ("nw", ResizeEdge::NorthWest),
        ("se", ResizeEdge::SouthEast),
        ("sw", ResizeEdge::SouthWest),
    ];
    rsx! {
        for (name, edge) in edges {
            div { class: "mk-resize mk-resize-{name}", onmousedown: move |_| controls.resize.call(edge) }
        }
    }
}
