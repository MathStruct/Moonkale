//! The editor panel: toolbar + backend host.
//!
//! State lives in the workspace's `Document` signal; this component holds
//! only the mounted backend. Docking the panel elsewhere remounts it, which
//! re-mounts the backend with the document's current text — nothing is lost.

use crate::backend::{self, BackendEvent, CodeEditorBackend};
use crate::lsp::LspManager;
use dioxus::prelude::*;
use moonkale_core::{NodeId, SourceError};
use moonkale_ext_api::{Command, Workspace};

const PANEL_CSS: Asset = asset!("/assets/panel.css");

#[component]
pub fn CodeEditorPanel(ws: Workspace, node: NodeId, lsp: LspManager) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let element_id = format!("mk-editor-{node}");
    let mut backend: Signal<Option<Box<dyn CodeEditorBackend>>> = use_signal(|| None);
    // LSP identity: language + folder root + file URI, when the file lives in a folder.
    let lsp_ident: Option<(String, String, String)> = {
        let d = doc.read();
        match (
            d.node.language_hint(),
            d.node.source.as_str().strip_prefix("folder:"),
        ) {
            (Some(lang), Some(root)) => Some((
                lang.to_string(),
                root.to_string(),
                format!("file://{root}/{}", d.node.native_key),
            )),
            _ => None,
        }
    };
    let mut lsp_version = use_signal(|| 1i32);
    let mut lsp_session: Signal<Option<moonkale_lsp::LspSession>> = use_signal(|| None);
    let mut ready = use_signal(|| false);
    let mut last_error: Signal<Option<SourceError>> = use_signal(|| None);

    // Mount the backend once the host element exists (after first render).
    use_effect({
        let element_id = element_id.clone();
        let lsp_ident = lsp_ident.clone();
        move || {
            if backend.read().is_some() {
                return;
            }
            let initial = doc.peek().text.clone();
            let ident = lsp_ident.clone();
            let on_event = Callback::new(move |ev: BackendEvent| match ev {
                BackendEvent::Ready => ready.set(true),
                BackendEvent::Changed(text) => {
                    doc.with_mut(|d| d.text = text.clone());
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.as_ref())
                    {
                        let v = *lsp_version.peek() + 1;
                        lsp_version.set(v);
                        s.did_change(uri, v, &text);
                    }
                }
                BackendEvent::Hover { id, line, col } => {
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        spawn(async move {
                            let text = s.hover(&uri, line, col).await.ok().flatten();
                            if let Some(b) = backend.peek().as_ref() {
                                b.hover_result(id, text.as_deref());
                            }
                        });
                    } else if let Some(b) = backend.peek().as_ref() {
                        b.hover_result(id, None);
                    }
                }
                BackendEvent::Definition { line, col } => {
                    if let (Some(s), Some((_, root, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        spawn(async move {
                            match s.definition(&uri, line, col).await {
                                Ok(Some(loc)) if loc.uri == uri => {
                                    if let Some(b) = backend.peek().as_ref() {
                                        b.set_cursor(loc.line, loc.col);
                                    }
                                }
                                Ok(Some(loc)) => {
                                    let prefix = format!("file://{root}/");
                                    if let Some(rel) = loc.uri.strip_prefix(&prefix) {
                                        let _ = ws.open_relative_path(rel).await;
                                    } else {
                                        let mut ws = ws;
                                        ws.set_status(format!(
                                            "Definition is outside the folder: {}",
                                            loc.uri
                                        ));
                                    }
                                }
                                Ok(None) => {
                                    let mut ws = ws;
                                    ws.set_status("No definition found");
                                }
                                Err(e) => {
                                    let mut ws = ws;
                                    ws.set_status(format!("definition: {e}"));
                                }
                            }
                        });
                    }
                }
            });
            // Language server: attach (starting one if needed) and announce the document.
            if let Some((lang, root, uri)) = lsp_ident.clone() {
                spawn(async move {
                    if let Some(s) = lsp.ensure(ws, &lang, &root).await {
                        let text = doc.peek().text.clone();
                        s.did_open(&uri, &lang, 1, &text);
                        lsp_session.set(Some(s));
                    }
                });
            }
            backend.set(Some(backend::mount(element_id.clone(), initial, on_event)));
        }
    });

    // Diagnostics from the language server → decorations in the view.
    {
        let uri = lsp_ident.as_ref().map(|(_, _, u)| u.clone());
        use_effect(move || {
            let Some(uri) = uri.clone() else { return };
            let items = lsp
                .diagnostics
                .read()
                .get(&uri)
                .cloned()
                .unwrap_or_default();
            if !ready() {
                return;
            }
            if let Some(b) = backend.peek().as_ref() {
                b.set_diagnostics(&items);
            }
        });
    }
    // Tell the server when the document is saved or closed.
    {
        let uri = lsp_ident.as_ref().map(|(_, _, u)| u.clone());
        use_drop(move || {
            if let (Some(s), Some(uri)) = (lsp_session.peek().clone(), uri.as_ref()) {
                s.did_close(uri);
            }
        });
    }

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

    let saved_uri = lsp_ident.as_ref().map(|(_, _, u)| u.clone());
    let save = {
        let saved_uri = saved_uri.clone();
        move |_| {
            let saved_uri = saved_uri.clone();
            async move {
                match ws.save(node).await {
                    Ok(()) => {
                        last_error.set(None);
                        if let (Some(s), Some(uri)) = (lsp_session.peek().clone(), saved_uri) {
                            s.did_save(&uri);
                        }
                    }
                    Err(e) => last_error.set(Some(e)),
                }
            }
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
                span { class: "mk-editor-path", "{title}" }
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
