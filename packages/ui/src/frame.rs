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

    // User settings (recent folders, provider, …) from the platform store,
    // started from the frame's `onmounted` (an event, so the webview is up)
    // — on desktop this also reopens the last folder.
    let mut settings_started = use_signal(|| false);
    let start_settings = move |_| {
        if *settings_started.peek() {
            return;
        }
        settings_started.set(true);
        spawn(async move { ws.load_user_settings().await });
    };

    // Shortcuts when nothing inside the frame has focus (P-065): a
    // document-level listener forwards Ctrl-combos whose target is the body.
    use_hook(move || {
        let mut ev = document::eval(GLOBAL_KEYS);
        spawn(async move {
            loop {
                match ev.recv::<(String, bool)>().await {
                    Ok((key, shift)) => {
                        shortcut(ws, &key, shift);
                    }
                    Err(dioxus::document::EvalError::Serialization(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    });

    // Open documents follow the workspace: restore them when a folder's
    // settings load, save the list (and the active one) whenever it changes.
    let mut restored_docs_for: Signal<Option<moonkale_core::SourceId>> = use_signal(|| None);
    use_effect(move || {
        let folder = ws.settings_folder.read().clone();
        let Some(folder) = folder else { return };
        if restored_docs_for.peek().as_ref() == Some(&folder) {
            return;
        }
        restored_docs_for.set(Some(folder.clone()));
        let (keys, active) = {
            let s = ws.settings.peek();
            (s.open_documents.clone(), s.active_document.clone())
        };
        if keys.is_empty() {
            return;
        }
        tracing::info!("settings: restoring {} documents", keys.len());
        spawn(async move {
            let mut activate = None;
            for key in keys {
                if let Some(node) = ws.node_at_path(&folder, &key).await {
                    if active.as_deref() == Some(key.as_str()) {
                        activate = Some(node.id);
                    }
                    let _ = ws.open_node(node).await;
                }
            }
            if let Some(id) = activate {
                ws.active.set(Some(id));
            }
        });
    });
    use_effect(move || {
        let docs = ws.documents.read();
        let active = *ws.active.read();
        let Some(folder) = ws.settings_folder.peek().clone() else {
            return;
        };
        if restored_docs_for.peek().as_ref() != Some(&folder) {
            return;
        }
        let keys: Vec<String> = docs
            .iter()
            .filter_map(|(_, d)| {
                let d = d.peek();
                (d.node.source == folder).then(|| d.node.native_key.clone())
            })
            .collect();
        let active_key = active.and_then(|id| {
            docs.iter().find(|(n, _)| *n == id).and_then(|(_, d)| {
                let d = d.peek();
                (d.node.source == folder).then(|| d.node.native_key.clone())
            })
        });
        drop(docs);
        let current = ws.settings_workspace.peek();
        if current.open_documents == keys && current.active_document == active_key {
            return;
        }
        drop(current);
        spawn(async move {
            ws.update_workspace_settings(move |f| {
                f.open_documents = keys;
                f.active_document = active_key;
            })
            .await;
        });
    });

    // Frame-level commands.
    use_effect(move || {
        let (_, cmd) = *ws.commands.read();
        match cmd {
            Some(Command::NewWindow) => match config.new_window {
                Some(open) => open(),
                None => ws.set_status("New window is not available on this platform"),
            },
            Some(Command::OpenRecent(i)) => {
                let path = ws.settings.peek().recent_folders.get(i).cloned();
                if let Some(path) = path {
                    spawn(async move {
                        if let Err(e) = ws.open_folder(path).await {
                            ws.set_status(format!("Open failed: {e}"));
                        }
                    });
                }
            }
            Some(Command::Settings) => {
                ws.dispatch(Command::ShowPanel(crate::settings_panel::PANEL_ID))
            }
            _ => {}
        }
    });

    let foreign = ws.foreign_drag.read().clone();

    rsx! {
        div {
            class: "mk-frame",
            class: if controls.is_some() { "mk-frame-undecorated" },
            onmounted: start_settings,
            // Global keybindings. Ctrl+S is handled inside the editor panel
            // (it needs the panel's error state); the rest go through the bus.
            onkeydown: move |e| {
                let m = e.modifiers();
                if !(m.ctrl() || m.meta()) {
                    return;
                }
                let key = match e.key() { Key::Character(c) => c.to_ascii_lowercase(), _ => return };
                if shortcut(ws, &key, m.shift()) {
                    e.prevent_default();
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

/// The global shortcuts; `true` when handled. Ctrl+S stays inside the editor.
fn shortcut(mut ws: Workspace, key: &str, shift: bool) -> bool {
    match (key, shift) {
        ("w", false) => ws.dispatch(Command::CloseEditor),
        ("`", false) => {
            ws.terminal_cwd.set(None);
            ws.dispatch(Command::NewTerminal);
        }
        ("n", true) => ws.dispatch(Command::NewWindow),
        ("o", false) => ws.dispatch(Command::OpenFolder),
        ("f", true) => crate::search::focus_search(ws),
        (",", false) => ws.dispatch(Command::Settings),
        _ => return false,
    }
    true
}

/// Forwards Ctrl/Cmd-combos to Rust only when the event would otherwise be
/// lost (target is the document body, i.e. nothing focused inside the frame).
const GLOBAL_KEYS: &str = r#"
const keys = new Set(["w", "`", "n", "o", "f", ","]);
document.addEventListener("keydown", (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (!keys.has(k)) return;
    const t = e.target;
    if (t && t !== document.body && t !== document.documentElement) return;
    e.preventDefault();
    dioxus.send([k, e.shiftKey]);
});
for (;;) { await dioxus.recv(); }
"#;
