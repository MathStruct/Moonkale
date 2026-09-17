//! The editor panel: toolbar + backend host.
//!
//! State lives in the workspace's `Document` signal; this component holds
//! only the mounted backend. Docking the panel elsewhere remounts it, which
//! re-mounts the backend with the document's current text — nothing is lost.

use crate::backend::{self, BackendEvent, CodeEditorBackend};
use dioxus::prelude::*;
use moonkale_core::{NodeId, SourceError};
use moonkale_ext_api::{Command, Workspace};

const PANEL_CSS: Asset = asset!("/assets/panel.css");

#[component]
pub fn CodeEditorPanel(ws: Workspace, node: NodeId) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let element_id = format!("mk-editor-{node}");
    let handle_id = format!("mk-drag-{node}");
    let mut backend: Signal<Option<Box<dyn CodeEditorBackend>>> = use_signal(|| None);
    let mut ready = use_signal(|| false);
    let mut last_error: Signal<Option<SourceError>> = use_signal(|| None);

    // Mount the backend once the host element exists (after first render).
    use_effect({
        let element_id = element_id.clone();
        let handle_id = handle_id.clone();
        move || {
            if backend.read().is_some() {
                return;
            }
            // Firefox only starts an HTML5 drag if `dragstart` sets data, and
            // Dioxus's synthetic event can't; a native listener does it.
            document::eval(&format!(
                r#"const h = document.getElementById({id});
                   if (h) h.addEventListener("dragstart", (e) => {{
                       e.dataTransfer.setData("text/plain", "moonkale-document:" + {id});
                       e.dataTransfer.effectAllowed = "move";
                   }});"#,
                id = serde_json::to_string(&handle_id).unwrap()
            ));
            let initial = doc.peek().text.clone();
            let on_event = Callback::new(move |ev: BackendEvent| match ev {
                BackendEvent::Ready => ready.set(true),
                BackendEvent::Changed(text) => doc.with_mut(|d| d.text = text),
            });
            backend.set(Some(backend::mount(element_id.clone(), initial, on_event)));
        }
    });

    // Application commands aimed at the active editor (menus, keybindings).
    use_effect(move || {
        let (_, cmd) = *ws.commands.read();
        if ws.active.peek().as_ref() != Some(&node) {
            return;
        }
        match cmd {
            Some(Command::Save) => {
                spawn(async move {
                    match ws.save(node).await {
                        Ok(()) => last_error.set(None),
                        Err(e) => last_error.set(Some(e)),
                    }
                });
            }
            Some(Command::Undo) => {
                if let Some(b) = backend.peek().as_ref() {
                    b.undo();
                }
            }
            Some(Command::Redo) => {
                if let Some(b) = backend.peek().as_ref() {
                    b.redo();
                }
            }
            Some(Command::CloseEditor) => ws.close_node(node),
            _ => {}
        }
    });

    let save = move |_| async move {
        match ws.save(node).await {
            Ok(()) => last_error.set(None),
            Err(e) => last_error.set(Some(e)),
        }
    };
    let reload = move |_| async move {
        if ws.reload(node).await.is_ok() {
            if let Some(b) = backend.read().as_ref() {
                b.set_text(&doc.peek().text);
            }
            last_error.set(None);
        }
    };

    let mut ws = ws;
    let d = doc.read();
    let dirty = d.dirty();
    let title = d.node.native_key.clone();
    let lang = d.node.language_hint().unwrap_or("plain text");
    let version = d.version;
    drop(d);

    rsx! {
        document::Stylesheet { href: PANEL_CSS }
        document::Script { src: backend::codemirror::BUNDLE, defer: true }
        div {
            class: "mk-editor",
            // Ctrl+S / Cmd+S anywhere in the panel, including inside CodeMirror.
            onkeydown: move |e| {
                let mods = e.modifiers();
                if (mods.ctrl() || mods.meta()) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    spawn(async move {
                        match ws.save(node).await {
                            Ok(()) => last_error.set(None),
                            Err(err) => last_error.set(Some(err)),
                        }
                    });
                }
            },
            div { class: "mk-editor-toolbar",
                // Drag this label into another window of the session to move the document there.
                span {
                    id: "{handle_id}",
                    class: "mk-editor-path",
                    draggable: true,
                    title: "Drag into another Moonkale window to move this editor there",
                    ondragstart: move |_| ws.start_drag(node),
                    ondragend: move |_| ws.end_drag(),
                    "⋮⋮ {title}"
                }
                if dirty { span { class: "mk-editor-dirty", title: "Unsaved changes", "●" } }
                span { class: "mk-editor-spacer" }
                span { class: "mk-editor-meta", "{lang} · {version}" }
                button { class: "mk-btn", disabled: !dirty, onclick: save, "Save" }
                button { class: "mk-btn", onclick: reload, title: "Discard edits and reload from the source", "Reload" }
            }
            if let Some(err) = last_error() {
                div { class: "mk-editor-error",
                    match err {
                        SourceError::Conflict { .. } => "The file changed outside the editor. Reload to see the new content (your edits will be lost) or save again to overwrite.".to_string(),
                        other => format!("Save failed: {other}"),
                    }
                }
            }
            div { id: "{element_id}", class: "mk-editor-host",
                if !ready() { div { class: "mk-editor-loading", "Loading editor…" } }
            }
        }
    }
}
