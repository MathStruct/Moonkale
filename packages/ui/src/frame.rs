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

const WASM_HOST_JS: Asset = asset!("/assets/wasm_host.js");

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
    let exts = use_context_provider(|| Extensions_(Rc::new((config.extensions)()))).0;
    // The command registry (Milestone 7): rebuilt when settings (keybindings,
    // enabled extensions) change, read by menus, keys and the palette.
    let mut registry: crate::commands::CommandRegistry =
        use_context_provider(|| Signal::new(Rc::new(crate::commands::Registry::default())));
    let mut palette: crate::palette::PaletteState = use_context_provider(|| Signal::new(None));
    {
        let exts = exts.clone();
        use_effect(move || {
            let next = crate::commands::Registry::build(&exts, ws);
            if **registry.peek() != next {
                registry.set(Rc::new(next));
            }
        });
    }

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
                // Explorer rows (Milestone 7): Firefox starts a drag only when
                // the transfer carries data.
                const row = e.target && e.target.closest ? e.target.closest(".mk-tree-row[draggable=true]") : null;
                if (row && e.dataTransfer) { e.dataTransfer.setData("text/plain", row.title || "row"); e.dataTransfer.effectAllowed = "move"; }
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
                match ev.recv::<(String, bool, bool, bool)>().await {
                    Ok((key, ctrl, shift, alt)) => {
                        crate::commands::handle_key(ws, ctrl, shift, alt, &key);
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

    // Presence (Milestone 8): tell the hub whenever the active document or
    // our name changes.
    use_effect(move || {
        let _ = ws.active.read();
        let _ = ws.settings.read().user_name.clone();
        ws.publish_presence();
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
            Some(Command::Palette) => palette.set(Some(crate::palette::PaletteMode::Commands)),
            Some(Command::QuickOpen) => palette.set(Some(crate::palette::PaletteMode::Files)),
            Some(Command::SearchWorkspace) => crate::search::focus_search(ws),
            Some(Command::NewFile(name, template)) => {
                let folder = ws
                    .sources
                    .peek()
                    .iter()
                    .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
                    .cloned();
                let Some(folder) = folder else {
                    ws.set_status("Open a folder first");
                    return;
                };
                spawn(async move {
                    let (stem, ext) = match name.find('.') {
                        Some(i) => (&name[..i], &name[i..]),
                        None => (name, ""),
                    };
                    let mut candidate = name.to_string();
                    let mut n = 1;
                    while ws
                        .node_at_path(&folder.descriptor.id, &candidate)
                        .await
                        .is_some()
                    {
                        n += 1;
                        candidate = format!("{stem}-{n}{ext}");
                    }
                    match ws
                        .create_text(
                            &folder.descriptor.id,
                            folder.descriptor.root,
                            &candidate,
                            template,
                        )
                        .await
                    {
                        Ok(node) => {
                            let _ = ws.open_node(node).await;
                        }
                        Err(e) => ws.set_status(format!("Could not create {candidate}: {e}")),
                    }
                });
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
            // Global keybindings through the command registry. Editors
            // handle Ctrl+S themselves and stop the event (they need their
            // own error state); everything else that bubbles here is looked up.
            onkeydown: move |e| {
                let m = e.modifiers();
                let key = key_name(&e.key());
                if key.is_empty() {
                    return;
                }
                if crate::commands::handle_key(ws, m.ctrl() || m.meta(), m.shift(), m.alt(), &key) {
                    e.prevent_default();
                    e.stop_propagation();
                }
            },
            // The browser wasm runtime (Milestone 8); harmless where unused.
            document::Script { src: WASM_HOST_JS, defer: true }
            TitleBar { controls }
            div { class: "mk-frame-body", {children} }
            crate::palette::Palette {}
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
/// `KeyboardEvent.key` spelling for the registry: characters as typed
/// (case-folded by the matcher), named keys by their DOM name.
fn key_name(key: &Key) -> String {
    match key {
        Key::Character(c) => c.clone(),
        Key::Enter => "Enter".into(),
        Key::Escape => "Escape".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "Backspace".into(),
        Key::Delete => "Delete".into(),
        Key::ArrowUp => "ArrowUp".into(),
        Key::ArrowDown => "ArrowDown".into(),
        Key::ArrowLeft => "ArrowLeft".into(),
        Key::ArrowRight => "ArrowRight".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PageUp".into(),
        Key::PageDown => "PageDown".into(),
        Key::F1 => "F1".into(),
        Key::F2 => "F2".into(),
        Key::F3 => "F3".into(),
        Key::F4 => "F4".into(),
        Key::F5 => "F5".into(),
        Key::F6 => "F6".into(),
        Key::F7 => "F7".into(),
        Key::F8 => "F8".into(),
        Key::F9 => "F9".into(),
        Key::F10 => "F10".into(),
        Key::F11 => "F11".into(),
        Key::F12 => "F12".into(),
        _ => String::new(),
    }
}

/// Forwards Ctrl/Cmd-combos to Rust only when the event would otherwise be
/// lost (target is the document body, i.e. nothing focused inside the frame).
const GLOBAL_KEYS: &str = r#"
document.addEventListener("keydown", (e) => {
    const fkey = /^F\d{1,2}$/.test(e.key);
    if (!(e.ctrlKey || e.metaKey || e.altKey || fkey)) return;
    const t = e.target;
    if (t && t !== document.body && t !== document.documentElement) return;
    // Let the browser keep its own essentials (reload, devtools, tabs).
    if ((e.ctrlKey || e.metaKey) && ["r", "t", "l", "c", "v", "x", "a", "z", "y"].includes(e.key.toLowerCase()) && !e.shiftKey) return;
    if (e.key === "F5" || e.key === "F12") return;
    e.preventDefault();
    dioxus.send([e.key, e.ctrlKey || e.metaKey, e.shiftKey, e.altKey]);
});
for (;;) { await dioxus.recv(); }
"#;
