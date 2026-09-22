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
    // The view's JavaScript failed to mount (spec 016): shown in place of the editor.
    let mut mount_error: Signal<Option<String>> = use_signal(|| None);
    // Milestone 7: an F2 rename prompt, offered code actions, references.
    let mut rename_prompt: Signal<Option<(u32, u32, String)>> = use_signal(|| None);
    let mut actions: Signal<Option<Vec<moonkale_lsp::CodeAction>>> = use_signal(|| None);
    let mut references: Signal<Option<Vec<moonkale_lsp::Location>>> = use_signal(|| None);
    // What the view currently shows (its own edits, or text we pushed), so
    // text changed elsewhere (agent `editor.replace`, reload) is pushed in.
    let mut view_text: Signal<String> = use_signal(|| doc.peek().text.clone());

    // Mount the backend from the host element's `onmounted` — not an effect:
    // on desktop the DOM mutation reaches the webview asynchronously, so an
    // eval started from an effect can run before the element exists and
    // return silently, leaving "Loading editor…" forever (P-047, spec 016).
    let mount = {
        let element_id = element_id.clone();
        let lsp_ident = lsp_ident.clone();
        move |_: MountedEvent| {
            if backend.peek().is_some() {
                return;
            }
            let initial = doc.peek().text.clone();
            let ident = lsp_ident.clone();
            let element_id_for_events = element_id.clone();
            let on_event = Callback::new(move |ev: BackendEvent| match ev {
                BackendEvent::Ready => ready.set(true),
                BackendEvent::Failed(message) => mount_error.set(Some(message)),
                // Spec 018 / P-037: the view sends splices; `view_text` is
                // Rust's mirror of the view and receives them first, then
                // the document takes the mirror (a memcpy, not a JSON hop).
                BackendEvent::Spliced { changes, length } => {
                    let ok = view_text.with_mut(|t| backend::apply_splices(t, &changes));
                    let mirrored_len = view_text.peek().encode_utf16().count() as u32;
                    if !ok || mirrored_len != length {
                        // Out of sync (should not happen): the whole text is
                        // logged and the view is asked to replace ours.
                        tracing::warn!(
                            "editor: splice mismatch (ok={ok}, len {mirrored_len} vs {length}); resyncing"
                        );
                        let text = doc.peek().text.clone();
                        view_text.set(text.clone());
                        if let Some(b) = backend.peek().as_ref() {
                            b.set_text(&text);
                        }
                        return;
                    }
                    let text = view_text.peek().clone();
                    if doc.peek().text != text {
                        doc.with_mut(|d| d.text = text.clone());
                    }
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
                BackendEvent::Cursor { line, col } => {
                    let mut ws = ws;
                    ws.set_cursor(node, line, col);
                }
                // Spec 012: `[[` completion and Ctrl+click in markdown sources.
                BackendEvent::WikiQuery { id, query } => {
                    spawn(async move {
                        let items: Vec<moonkale_lsp::CompletionItem> = ws
                            .wiki_candidates(&query, 12)
                            .await
                            .into_iter()
                            .map(|c| moonkale_lsp::CompletionItem {
                                label: c.target,
                                kind: "text".into(),
                                detail: Some(c.key),
                                insert: None,
                                sort: None,
                            })
                            .collect();
                        if let Some(b) = backend.peek().as_ref() {
                            b.completion_result(id, Some(&items));
                        }
                    });
                }
                BackendEvent::WikiLink { target } => {
                    let from = doc.peek().node.clone();
                    spawn(async move {
                        let _ = ws.follow_wiki(&from, &target, true).await;
                    });
                }
                BackendEvent::Completion { id, line, col } => {
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        spawn(async move {
                            let items = match s.completion(&uri, line, col).await {
                                Ok(items) => Some(items),
                                Err(e) => {
                                    tracing::warn!("completion: {e}");
                                    None
                                }
                            };
                            tracing::debug!(
                                "completion: {} items",
                                items.as_ref().map(Vec::len).unwrap_or(0)
                            );
                            if let Some(b) = backend.peek().as_ref() {
                                b.completion_result(id, items.as_deref());
                            }
                        });
                    } else if let Some(b) = backend.peek().as_ref() {
                        b.completion_result(id, None);
                    }
                }
                BackendEvent::Rename { line, col, word } => {
                    if lsp_session.peek().is_some() {
                        rename_prompt.set(Some((line, col, word)));
                        ws.focus_element(&format!("{element_id_for_events}-rename"));
                    } else {
                        let mut ws = ws;
                        ws.set_status("Rename needs a language server for this file");
                    }
                }
                BackendEvent::CodeActions {
                    line,
                    col,
                    end_line,
                    end_col,
                } => {
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        spawn(async move {
                            match s.code_actions(&uri, line, col, end_line, end_col).await {
                                Ok(list) => actions.set(Some(list)),
                                Err(e) => {
                                    let mut ws = ws;
                                    ws.set_status(format!("code actions: {e}"));
                                }
                            }
                        });
                    }
                }
                BackendEvent::References { line, col } => {
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        spawn(async move {
                            match s.references(&uri, line, col).await {
                                Ok(list) => references.set(Some(list)),
                                Err(e) => {
                                    let mut ws = ws;
                                    ws.set_status(format!("references: {e}"));
                                }
                            }
                        });
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
                                        // Open the target and place the cursor
                                        // there (Workspace::reveal, M4).
                                        if let Ok(node) = ws.open_relative_path(rel).await {
                                            let _ = ws.reveal(node, loc.line, loc.col).await;
                                        }
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
            let language = doc.peek().node.language_hint().map(str::to_string);
            let wrap = ws.settings.peek().editor.wrap;
            backend.set(Some(backend::mount(
                element_id.clone(),
                initial,
                language,
                wrap,
                on_event,
            )));
        }
    };

    // Text changed outside the view (an agent edit, a reload): push it. The
    // mirror is *not* updated here — the view answers with the splice it
    // applied, and that brings the mirror up to date (spec 018).
    use_effect(move || {
        let text = doc.read().text.clone();
        if !ready() || *view_text.peek() == text {
            return;
        }
        if let Some(b) = backend.peek().as_ref() {
            b.set_text(&text);
        }
    });

    // A pending `Workspace::reveal` for this document: place the cursor once
    // the view is ready (search hits, trace frames, go-to-definition).
    {
        let node_id = node;
        let mut applied = use_signal(|| 0u64);
        use_effect(move || {
            let Some(r) = *ws.reveal.read() else { return };
            if r.node != node_id || !ready() || *applied.peek() == r.seq {
                return;
            }
            if let Some(b) = backend.peek().as_ref() {
                b.set_cursor(r.line, r.col);
                applied.set(r.seq);
            }
        });
    }
    // Word wrap follows the user setting (spec 014).
    {
        use_effect(move || {
            let wrap = ws.settings.read().editor.wrap;
            if !ready() {
                return;
            }
            if let Some(b) = backend.peek().as_ref() {
                b.set_wrap(wrap);
            }
        });
    }
    // Other people's cursors in this document (Milestone 9).
    {
        let key = doc.peek().node.native_key.clone();
        use_effect(move || {
            let _ = ws.presence.read();
            let marks: Vec<(u32, String)> = ws
                .others()
                .into_iter()
                .filter(|m| m.active.as_deref() == Some(key.as_str()))
                .filter_map(|m| m.line.map(|l| (l, m.initials())))
                .collect();
            if !ready() {
                return;
            }
            if let Some(b) = backend.peek().as_ref() {
                b.set_presence(&marks);
            }
        });
    }

    // `[[links]]` in markdown sources → decorations (spec 012): re-checked
    // 300 ms after the last edit and when the index changes.
    if doc.peek().node.language_hint() == Some("markdown") {
        let mut wiki_epoch = use_signal(|| 0u64);
        use_effect(move || {
            let _ = doc.read().text.len();
            let _ = ws.graph_epoch.read();
            if !ready() {
                return;
            }
            let epoch = wiki_epoch.peek().wrapping_add(1);
            wiki_epoch.set(epoch);
            spawn(async move {
                futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
                if *wiki_epoch.peek() != epoch {
                    return;
                }
                let (from, text) = {
                    let d = doc.peek();
                    (d.node.clone(), d.text.clone())
                };
                let spans = ws.wiki_spans(&from, &text).await;
                // Byte offsets → UTF-16 units, what the view counts in.
                let utf16_at = |byte: usize| text[..byte].encode_utf16().count() as u32;
                let marks: Vec<backend::WikiMark> = spans
                    .iter()
                    .map(|sp| backend::WikiMark {
                        from: utf16_at(sp.start),
                        to: utf16_at(sp.end),
                        resolved: sp.resolved,
                    })
                    .collect();
                if let Some(b) = backend.peek().as_ref() {
                    b.set_wiki_links(&marks);
                }
            });
        });
    }

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

    // Save from any path (Ctrl+S, the menu, the toolbar): write through the
    // source and tell the language server, whose checks run on save.
    let save_now = {
        let uri = lsp_ident.as_ref().map(|(_, _, u)| u.clone());
        Callback::new(move |_: ()| {
            let uri = uri.clone();
            spawn(async move {
                match ws.save(node).await {
                    Ok(()) => {
                        last_error.set(None);
                        if let (Some(s), Some(uri)) = (lsp_session.peek().clone(), uri) {
                            s.did_save(&uri);
                        }
                    }
                    Err(e) => last_error.set(Some(e)),
                }
            });
        })
    };

    // Application commands aimed at the active editor (menus, keybindings).
    use_effect(move || {
        let (_, cmd) = *ws.commands.read();
        if ws.active.peek().as_ref() != Some(&node) {
            return;
        }
        match cmd {
            Some(Command::Save) => {
                save_now.call(());
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
            Some(Command::Editor(action)) => {
                if let Some(b) = backend.peek().as_ref() {
                    b.run(action);
                }
            }
            _ => {}
        }
    });

    // Apply a server edit (rename, code action) to every file it touches:
    // open documents take it unsaved, closed files are written through the
    // source; the current document is pushed to the view by the text effect.
    let root_for_edits = lsp_ident.as_ref().map(|(_, r, _)| r.clone());
    let apply_edit = Callback::new(move |edit: moonkale_lsp::WorkspaceEdit| {
        let root = root_for_edits.clone();
        spawn(async move {
            let Some(root) = root else { return };
            match crate::lsp::apply_workspace_edit(ws, &root, &edit).await {
                Ok(n) => {
                    let mut ws = ws;
                    ws.set_status(format!("Applied edits to {n} file(s)"));
                }
                Err(e) => {
                    let mut ws = ws;
                    ws.set_status(format!("Edit failed: {e}"));
                }
            }
        });
    });
    let rename_ident = lsp_ident.clone();
    let mut rename_value = use_signal(String::new);
    let commit_rename = Callback::new(move |_: ()| {
        let Some((line, col, _)) = rename_prompt.peek().clone() else {
            return;
        };
        let new_name = rename_value.peek().trim().to_string();
        rename_prompt.set(None);
        if new_name.is_empty() {
            return;
        }
        if let (Some(s), Some((_, _, uri))) = (lsp_session.peek().clone(), rename_ident.clone()) {
            spawn(async move {
                match s.rename(&uri, line, col, &new_name).await {
                    Ok(edit) if edit.changes.is_empty() => {
                        let mut ws = ws;
                        ws.set_status("Nothing to rename here");
                    }
                    Ok(edit) => apply_edit.call(edit),
                    Err(e) => {
                        let mut ws = ws;
                        ws.set_status(format!("rename: {e}"));
                    }
                }
            });
        }
    });
    let rename_id = format!("{element_id}-rename");
    let root_for_refs = lsp_ident.as_ref().map(|(_, r, _)| r.clone());

    let save = move |_| save_now.call(());
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
    let wrap_on = ws.settings.read().editor.wrap;
    let version = d.version;
    drop(d);

    rsx! {
        moonkale_ext_api::Stylesheet { href: PANEL_CSS }
        document::Script { src: backend::codemirror::BUNDLE, defer: true }
        div {
            class: "mk-editor",
            // Ctrl+S / Cmd+S anywhere in the panel, including inside CodeMirror.
            onkeydown: move |e| {
                let mods = e.modifiers();
                if moonkale_ext_api::keys::primary(&mods) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    e.stop_propagation(); // the frame would dispatch Save again
                    save_now.call(());
                }
            },
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{title}" }
                if dirty { span { class: "mk-editor-dirty", title: "Unsaved changes", "●" } }
                span { class: "mk-editor-spacer" }
                span { class: "mk-editor-meta", "{lang} · {version}" }
                button {
                    class: if wrap_on { "mk-btn mk-btn-on" } else { "mk-btn" },
                    title: "Wrap long lines (Alt+Z)",
                    onclick: move |_| crate::extension::toggle_wrap(ws),
                    "Wrap"
                }
                button { class: "mk-btn", disabled: !dirty, onclick: save, "Save" }
                button { class: "mk-btn", onclick: reload, title: "Discard edits and reload from the source", "Reload" }
                // Milestone 14: move this document to the Rust editor.
                if ws.settings.read().extensions.is_enabled_id("dev.moonkale.editor-code-native", false) {
                    button { class: "mk-btn mk-editor-switch", title: "Show this file in the Rust editor (dioxus-code-editor)", onclick: move |_| { let mut ws = ws; ws.choose_editor(node, "native"); }, "Rust" }
                }
            }
            if let Some((_, _, word)) = rename_prompt() {
                div { class: "mk-editor-bar mk-editor-rename",
                    span { "Rename " code { "{word}" } " to:" }
                    input { id: "{rename_id}", class: "mk-input", value: "{rename_value}",
                        onmounted: { let w = word.clone(); move |_| rename_value.set(w.clone()) },
                        oninput: move |e| rename_value.set(e.value()),
                        onkeydown: move |e| {
                            e.stop_propagation();
                            match e.key() {
                                Key::Enter => { e.prevent_default(); commit_rename.call(()); }
                                Key::Escape => { e.prevent_default(); rename_prompt.set(None); }
                                _ => {}
                            }
                        },
                    }
                    button { class: "mk-btn", onclick: move |_| commit_rename.call(()), "Rename" }
                    button { class: "mk-btn", onclick: move |_| rename_prompt.set(None), "Cancel" }
                }
            }
            if let Some(list) = actions() {
                div { class: "mk-editor-bar mk-editor-actions", "data-count": "{list.len()}",
                    if list.is_empty() { span { class: "mk-muted", "No code actions here." } } else { span { "Code actions:" } }
                    for (i, a) in list.iter().enumerate() {
                        button { key: "{i}", class: "mk-btn", title: "{a.kind}", onclick: {
                            let a = a.clone();
                            move |_| {
                                actions.set(None);
                                let a = a.clone();
                                if let Some(s) = lsp_session.peek().clone() {
                                    spawn(async move {
                                        match s.resolve_code_action(&a).await {
                                            Ok(edit) => apply_edit.call(edit),
                                            Err(e) => { let mut ws = ws; ws.set_status(format!("code action: {e}")); }
                                        }
                                    });
                                }
                            }
                        }, "{a.title}" }
                    }
                    button { class: "mk-btn", onclick: move |_| actions.set(None), "✕" }
                }
            }
            if let Some(list) = references() {
                div { class: "mk-editor-bar mk-editor-refs", "data-count": "{list.len()}",
                    span { "{list.len()} reference(s)" }
                    button { class: "mk-btn", onclick: move |_| references.set(None), "✕" }
                    ul {
                        for (i, l) in list.iter().enumerate() {
                            li { key: "{i}", class: "mk-editor-ref", onclick: {
                                let l = l.clone();
                                let root = root_for_refs.clone();
                                move |_| {
                                    let l = l.clone();
                                    let root = root.clone();
                                    spawn(async move {
                                        let Some(root) = root else { return };
                                        if let Some(rel) = l.uri.strip_prefix(&format!("file://{root}/")) {
                                            if let Ok(n) = ws.open_relative_path(rel).await {
                                                let _ = ws.reveal(n, l.line, l.col).await;
                                            }
                                        }
                                    });
                                }
                            },
                                { root_for_refs.as_ref().and_then(|r| l.uri.strip_prefix(&format!("file://{r}/"))).unwrap_or(&l.uri).to_string() }
                                span { class: "mk-muted", ":{l.line + 1}:{l.col + 1}" }
                            }
                        }
                    }
                }
            }
            if let Some(err) = last_error() {
                div { class: "mk-editor-error",
                    match err {
                        SourceError::Conflict { .. } => "The file changed outside the editor. Reload to see the new content (your edits will be lost) or save again to overwrite.".to_string(),
                        other => format!("Save failed: {other}"),
                    }
                }
            }
            div { id: "{element_id}", class: "mk-editor-host", onmounted: mount,
                if let Some(err) = mount_error() {
                    div { class: "mk-editor-failed",
                        p { b { "The editor could not start." } " This is a bug — please report it with the message below (the file itself is fine; Reload retries)." }
                        pre { "{err}" }
                    }
                } else if !ready() {
                    div { class: "mk-editor-loading", "Loading editor…" }
                }
            }
        }
    }
}
