//! `Frame` — the outermost component: creates the workspace and extension
//! list once, provides both through context, renders the title bar, the
//! content, and (desktop only) invisible resize handles for the undecorated
//! window.

use crate::titlebar::TitleBar;
use dioxus::prelude::*;
use moonkale_ext_api::{
    Command, Extension, SessionBus, SessionMessage, Workspace, WorkspaceConfig,
};
use std::rc::Rc;

/// Creates the platform's session transport. It receives a callback to
/// deliver incoming messages into this window and returns the sender.
pub type SessionFactory = fn(Callback<SessionMessage>) -> Rc<dyn SessionBus>;

/// Builds the extension list. A plain `fn` so it can be a prop.
pub type Extensions = fn() -> Vec<Box<dyn Extension>>;

/// What the platform hands the shell. Set once at startup; comparing
/// function pointers is meaningless, so two configs are always "equal" and
/// the frame never re-mounts because of them.
#[derive(Clone, Copy)]
pub struct ShellConfig {
    pub extensions: Extensions,
    pub workspace: WorkspaceConfig,
    pub session: SessionFactory,
    /// Open another window/tab of this session (View → New Window).
    pub new_window: Option<fn()>,
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

    // Join the session: incoming messages are handled by the workspace.
    use_hook(move || {
        let deliver = Callback::new(move |msg: SessionMessage| {
            spawn(ws.handle_message(msg));
        });
        let bus = (config.session)(deliver);
        ws.connect_bus(bus);
    });

    // Native drag plumbing, once per window:
    // - a workbench tab is an HTML5 drag carrying its element id (Firefox needs
    //   data to start one at all); when a tab drag starts, start a session drag
    //   so the other windows offer to take the document;
    // - stray drops must never navigate the page (Firefox loads dropped text
    //   as a URL — "Server Not Found: wb-tab-editor-…").
    use_hook(move || {
        let mut rx = document::eval(
            r#"
            window.addEventListener("dragover", (e) => e.preventDefault());
            window.addEventListener("drop", (e) => e.preventDefault());
            document.addEventListener("dragstart", (e) => {
                const tab = e.target && e.target.closest ? e.target.closest(".wb-tab") : null;
                if (tab && tab.id) dioxus.send({ kind: "start", tab: tab.id });
            });
            // `dragend` is not always delivered (a drop on the workbench's own
            // drop zone swallows it in Firefox): a drop anywhere in this
            // window ends our drag as well.
            document.addEventListener("dragend", () => dioxus.send({ kind: "end" }));
            document.addEventListener("drop", () => dioxus.send({ kind: "end" }), true);
            "#,
        );
        spawn(async move {
            #[derive(serde::Deserialize)]
            #[serde(tag = "kind", rename_all = "lowercase")]
            enum DragMsg {
                Start { tab: String },
                End,
            }
            loop {
                match rx.recv::<DragMsg>().await {
                    Ok(DragMsg::Start { tab }) => {
                        ws.start_drag_from_tab(&tab);
                    }
                    Ok(DragMsg::End) => ws.end_drag(),
                    Err(dioxus::document::EvalError::Serialization(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    });

    // Frame-level commands.
    use_effect(move || {
        let (_, cmd) = *ws.commands.read();
        if cmd == Some(Command::NewWindow) {
            match config.new_window {
                Some(open) => open(),
                None => ws.set_status("New window is not available on this platform"),
            }
        }
    });

    let foreign = ws.foreign_drag.read().clone();

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
                    ("n", true) => {
                        e.prevent_default();
                        ws.dispatch(Command::NewWindow);
                    }
                    ("o", false) => { e.prevent_default(); ws.dispatch(Command::OpenFolder); }
                    _ => {}
                }
            },
            TitleBar { controls }
            div { class: "mk-frame-body", {children} }
            // Another window of this session is dragging a document: become a
            // drop target while the drag is live, and keep a banner afterwards
            // (an OS drag doesn't reach other windows on every platform).
            if let Some(drag) = foreign {
                if drag.live {
                    div {
                        class: "mk-drop-target",
                        ondragover: move |e| e.prevent_default(),
                        ondragenter: move |e| e.prevent_default(),
                        ondrop: move |e| {
                            e.prevent_default();
                            spawn(async move {
                                if let Err(err) = ws.accept_drop().await {
                                    ws.set_status(format!("Could not move document here: {err}"));
                                }
                            });
                        },
                        onclick: move |_| {
                            spawn(async move {
                                let _ = ws.accept_drop().await;
                            });
                        },
                        div { class: "mk-drop-target-label", "Drop (or click) to move " b { "{drag.node.native_key}" } " into this window" }
                    }
                } else {
                    div { class: "mk-drop-banner", role: "status",
                        span { b { "{drag.node.native_key}" } " was dragged from another window." }
                        button { class: "mk-btn mk-btn-accent", r#type: "button",
                            onclick: move |_| {
                                spawn(async move {
                                    if let Err(err) = ws.accept_drop().await {
                                        ws.set_status(format!("Could not move document here: {err}"));
                                    }
                                });
                            },
                            "Move it here"
                        }
                        button { class: "mk-btn", r#type: "button", title: "Dismiss", onclick: move |_| ws.dismiss_drop(), "✕" }
                    }
                }
            }
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
