//! The editor panel: toolbar + backend host.
//!
//! State lives in the workspace's `Document` signal; this component holds
//! only the mounted backend. Docking the panel elsewhere remounts it, which
//! re-mounts the backend with the document's current text — nothing is lost.

use crate::backend::{self, BackendEvent, CodeEditorBackend};
use crate::lsp::LspManager;
use crate::L;
use dioxus::prelude::*;
use moonkale_core::{NodeId, SourceError};
use moonkale_ext_api::{t, Command, Workspace};

const PANEL_CSS: Asset = asset!("/assets/panel.css");

#[component]
pub fn CodeEditorPanel(ws: Workspace, node: NodeId, lsp: LspManager) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let element_id = moonkale_ext_api::links::panel_element_id("mk-editor", node);
    let agent_view = moonkale_ext_api::links::panel_instance(format!("editor:{node}"))
        .starts_with("lead:agent:");
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
                crate::native_diagnostics::file_uri(root, &d.node.native_key),
            )),
            _ => None,
        }
    };
    let lsp_session = crate::lsp::use_document(ws, doc, lsp, lsp_ident.clone());
    let mut ready = use_signal(|| false);
    let mut last_error: Signal<Option<SourceError>> = use_signal(|| None);
    // The view's JavaScript failed to mount (spec 016): shown in place of the editor.
    let mut mount_error: Signal<Option<String>> = use_signal(|| None);
    // Milestone 7: an F2 rename prompt, offered code actions, references.
    let mut rename_prompt: Signal<Option<(u32, u32, String)>> = use_signal(|| None);
    let mut rename_base = use_signal(|| None::<(String, moonkale_lsp::LspSession)>);
    let mut actions: Signal<Option<Vec<moonkale_lsp::CodeAction>>> = use_signal(|| None);
    let mut actions_base = use_signal(|| None::<(String, moonkale_lsp::LspSession)>);
    let mut references: Signal<Option<Vec<moonkale_lsp::Location>>> = use_signal(|| None);
    // What the view currently shows (its own edits, or text we pushed), so
    // text changed elsewhere (agent `editor.replace`, reload) is pushed in.
    let binding = use_hook(move || crate::group_history::install(ws, node));
    let mut view_text: Signal<String> = use_signal(|| doc.peek().text.clone());
    // Splice mismatches in a row. A resync costs one more (the view answers
    // it with a splice against its old text), so a few are normal; one that
    // never settles stops the editor instead of looping (P-164).
    let mut mismatches = use_signal(|| 0u32);

    let selection = if *ws.docs.active.read() == Some(node) {
        *ws.docs.selection.read()
    } else {
        None
    };

    // Mount the backend from the host element's `onmounted` — not an effect:
    // on desktop the DOM mutation reaches the webview asynchronously, so an
    // eval started from an effect can run before the element exists and
    // return silently, leaving "Loading editor…" forever (P-047, spec 016).
    let mount = {
        let element_id = element_id.clone();
        let lsp_ident = lsp_ident.clone();
        let binding = binding.clone();
        move |_: MountedEvent| {
            if backend.peek().is_some() {
                return;
            }
            let initial = doc.peek().text.clone();
            let ident = lsp_ident.clone();
            let element_id_for_events = element_id.clone();
            let event_binding = binding.clone();
            let on_event = Callback::new(move |ev: BackendEvent| match ev {
                BackendEvent::Ready => ready.set(true),
                BackendEvent::Failed(message) => mount_error.set(Some(message)),
                // Spec 018 / P-037: the view sends splices; `view_text` is
                // Rust's mirror of the view and receives them first, then
                // the document takes the mirror (a memcpy, not a JSON hop).
                BackendEvent::GroupHistory { group, redo } => {
                    crate::group_history::restore(ws, group, redo)
                }
                BackendEvent::Spliced {
                    changes,
                    length,
                    stamp,
                } => {
                    let expected = event_binding.stamp.get();
                    if stamp.epoch != expected.epoch {
                        return;
                    }
                    if !stamp.workspace && stamp.sequence != expected.sequence.saturating_add(1) {
                        return;
                    }
                    event_binding.stamp.set(stamp);
                    let ok = view_text.with_mut(|t| backend::apply_splices(t, &changes));
                    let mirrored_len = view_text.peek().encode_utf16().count() as u32;
                    if !ok || mirrored_len != length {
                        // Out of sync (should not happen): the whole text is
                        // logged and the view is asked to replace ours.
                        tracing::warn!(
                            "editor: splice mismatch (ok={ok}, len {mirrored_len} vs {length}); resyncing"
                        );
                        let n = *mismatches.peek() + 1;
                        mismatches.set(n);
                        if n > 3 {
                            tracing::error!("editor: still out of sync after {n} resyncs; stopped");
                            mount_error.set(Some(t!(ws, L, "editor-out-of-sync")));
                            return;
                        }
                        let text = doc.peek().text.clone();
                        view_text.set(text.clone());
                        if let Some(b) = backend.peek().as_ref() {
                            b.set_text(&text);
                        }
                        return;
                    }
                    if *mismatches.peek() != 0 {
                        mismatches.set(0);
                    }
                    let text = view_text.peek().clone();
                    if !stamp.workspace && doc.peek().text != text {
                        doc.with_mut(|d| d.text = text.clone());
                    }
                    if stamp.workspace && doc.peek().text != text {
                        if let Some(b) = backend.peek().as_ref() {
                            b.set_text(&doc.peek().text);
                        }
                        return;
                    }
                    // A structural review may have created a Rust revision mirror.
                    // CodeMirror owns its own undo; external mirror adoption invalidates
                    // old proposals, while an echoed Workspace edit preserves history.
                    let mirror = ws.docs.editor_sessions.peek().get(&node).copied();
                    if let Some(mut mirror) = mirror {
                        let _ = mirror.with_mut(|state| state.replace_from_workspace(text.clone()));
                    }
                    if let Some((_, _, uri)) = ident.as_ref() {
                        lsp.update_document(uri, &text, &doc.peek().saved);
                    }
                }
                BackendEvent::Hover { id, line, col } => {
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        moonkale_ext_api::crash::spawn(async move {
                            let text = s.hover(&uri, line, col).await.ok().flatten();
                            if let Some(b) = backend.peek().as_ref() {
                                b.hover_result(id, text.as_deref());
                            }
                        });
                    } else if let Some(b) = backend.peek().as_ref() {
                        b.hover_result(id, None);
                    }
                }
                BackendEvent::Cursor {
                    line,
                    col,
                    anchor,
                    head,
                } => {
                    let mut ws = ws;
                    ws.set_selection(node, anchor, head);
                    ws.set_cursor(node, line, col);
                }
                // Spec 012: `[[` completion and Ctrl+click in markdown sources.
                BackendEvent::WikiQuery { id, query } => {
                    moonkale_ext_api::crash::spawn(async move {
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
                                filter: None,
                                text_edit: None,
                                additional_edits: Vec::new(),
                            })
                            .collect();
                        if let Some(b) = backend.peek().as_ref() {
                            b.completion_result(id, Some(&items));
                        }
                    });
                }
                BackendEvent::WikiLink { target } => {
                    let from = doc.peek().node.clone();
                    moonkale_ext_api::crash::spawn(async move {
                        let _ = ws.follow_wiki(&from, &target, true).await;
                    });
                }
                BackendEvent::Completion { id, line, col } => {
                    if let (Some(s), Some((_, _, uri))) =
                        (lsp_session.peek().clone(), ident.clone())
                    {
                        moonkale_ext_api::crash::spawn(async move {
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
                    if let Some(session) = lsp_session.peek().clone() {
                        rename_base.set(Some((doc.peek().text.clone(), session)));
                        rename_prompt.set(Some((line, col, word)));
                        ws.focus_element(&format!("{element_id_for_events}-rename"));
                    } else {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-rename-needs-lsp"));
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
                        let base = doc.peek().text.clone();
                        let diagnostics = lsp
                            .diagnostics
                            .peek()
                            .get(&uri)
                            .cloned()
                            .unwrap_or_default();
                        moonkale_ext_api::crash::spawn(async move {
                            match s
                                .code_actions_with_diagnostics(
                                    &uri,
                                    line,
                                    col,
                                    end_line,
                                    end_col,
                                    &diagnostics,
                                )
                                .await
                            {
                                Ok(list)
                                    if doc.peek().text == base
                                        && lsp
                                            .peek_document_session(&uri)
                                            .is_some_and(|current| s.same_connection(&current)) =>
                                {
                                    actions_base.set(Some((base, s)));
                                    actions.set(Some(list));
                                }
                                Ok(_) => {}
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
                        moonkale_ext_api::crash::spawn(async move {
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
                        moonkale_ext_api::crash::spawn(async move {
                            match s.definition(&uri, line, col).await {
                                Ok(Some(loc)) if loc.uri == uri => {
                                    if let Some(b) = backend.peek().as_ref() {
                                        b.set_cursor(loc.line, loc.col);
                                    }
                                }
                                Ok(Some(loc)) => {
                                    if let Some(rel) = crate::uri::relative_file(&root, &loc.uri) {
                                        // Open the target and place the cursor
                                        // there (Workspace::reveal, M4).
                                        if let Ok(node) = ws.open_relative_path(&rel).await {
                                            let _ = ws.reveal(node, loc.line, loc.col).await;
                                        }
                                    } else {
                                        let mut ws = ws;
                                        ws.set_status(t!(
                                            ws,
                                            L,
                                            "editor-definition-outside",
                                            uri = loc.uri.clone()
                                        ));
                                    }
                                }
                                Ok(None) => {
                                    let mut ws = ws;
                                    ws.set_status(t!(ws, L, "editor-no-definition"));
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
            let language = doc.peek().node.language_hint().map(str::to_string);
            let wrap = ws.settings.resolved.peek().editor.wrap;
            backend.set(Some(backend::mount_bound(
                element_id.clone(),
                initial,
                language,
                wrap,
                on_event,
                binding.clone(),
            )));
        }
    };

    moonkale_ext_api::crash::use_effect(move || {
        let _language = ws.lang();
        if !ready() {
            return;
        }
        if let Some(view) = backend.peek().as_ref() {
            view.set_phrases(std::collections::HashMap::from([
                ("Find".into(), t!(ws, L, "cm-search-find")),
                ("Replace".into(), t!(ws, L, "cm-search-replace")),
                ("next".into(), t!(ws, L, "cm-search-next")),
                ("previous".into(), t!(ws, L, "cm-search-previous")),
                ("all".into(), t!(ws, L, "cm-search-all")),
                ("match case".into(), t!(ws, L, "cm-search-case")),
                ("regexp".into(), t!(ws, L, "cm-search-regexp")),
                ("by word".into(), t!(ws, L, "cm-search-word")),
                ("replace".into(), t!(ws, L, "cm-search-replace-one")),
                ("replace all".into(), t!(ws, L, "cm-search-replace-all")),
                ("close".into(), t!(ws, L, "cm-search-close")),
                ("Go to line".into(), t!(ws, L, "cm-search-goto")),
                ("go".into(), t!(ws, L, "cm-search-go")),
                ("current match".into(), t!(ws, L, "cm-search-current")),
                ("replaced $ matches".into(), t!(ws, L, "cm-search-replaced")),
                (
                    "replaced match on line $".into(),
                    t!(ws, L, "cm-search-replaced-line"),
                ),
                ("on line".into(), t!(ws, L, "cm-search-on-line")),
            ]));
        }
    });

    // Text changed outside the view (an agent edit, a reload): push it. The
    // mirror is *not* updated here — the view answers with the splice it
    // applied, and that brings the mirror up to date (spec 018).
    moonkale_ext_api::crash::use_effect(move || {
        let text = doc.read().text.clone();
        if !ready() || *view_text.peek() == text {
            return;
        }
        let mut ws = ws;
        ws.clear_selection(node);
        if let Some(b) = backend.peek().as_ref() {
            b.set_text(&text);
        }
    });

    {
        let uri = lsp_ident.as_ref().map(|(_, _, uri)| uri.clone());
        moonkale_ext_api::crash::use_effect(move || {
            let document = doc.read();
            // The attachment signal makes this run again after an async open.
            if lsp_session.read().is_some() {
                if let Some(uri) = &uri {
                    lsp.update_document(uri, &document.text, &document.saved);
                }
            }
        });
    }

    // A pending `Workspace::reveal` for this document: place the cursor once
    // the view is ready (search hits, trace frames, go-to-definition).
    {
        let node_id = node;
        let mut applied = use_signal(|| 0u64);
        moonkale_ext_api::crash::use_effect(move || {
            let Some(r) = *ws.docs.reveal.read() else {
                return;
            };
            if agent_view || r.node != node_id || !ready() || *applied.peek() == r.seq {
                return;
            }
            if let Some(b) = backend.peek().as_ref() {
                b.set_cursor(r.line, r.col);
                applied.set(r.seq);
            }
        });
    }
    let agent_panel =
        use_hook(move || moonkale_ext_api::links::panel_instance(format!("editor:{node}")));
    let mut seen_agent_range = use_signal(|| None);
    moonkale_ext_api::crash::use_effect(move || {
        let range = ws.links.reveals.read().get(&agent_panel).cloned();
        if !ready() || range == *seen_agent_range.peek() {
            return;
        }
        if let (Some(range), Some(backend)) = (&range, backend.peek().as_ref()) {
            backend.show_range(
                range.line,
                range.column,
                range.end_line.unwrap_or(range.line),
                range.end_column.unwrap_or(range.column),
            );
            seen_agent_range.set(Some(range.clone()));
        }
    });
    // Word wrap follows the user setting (spec 014).
    {
        moonkale_ext_api::crash::use_effect(move || {
            let editor = ws.settings.resolved.read().editor.clone();
            if !ready() {
                return;
            }
            if let Some(b) = backend.peek().as_ref() {
                b.set_wrap(editor.wrap);
                let (default_spaces, default_width) = crate::native_language::indent_defaults(
                    crate::native_language::language_for_hint(doc.peek().node.language_hint()),
                );
                if editor.insert_spaces.is_some() || editor.indent_width.is_some() {
                    b.set_indentation(
                        Some(editor.insert_spaces.unwrap_or(default_spaces)),
                        Some(editor.indent_width.unwrap_or(default_width)),
                    );
                } else {
                    b.set_indentation(None, None);
                }
            }
        });
    }
    // Other people's cursors in this document (Milestone 9).
    {
        let key = doc.peek().node.native_key.clone();
        moonkale_ext_api::crash::use_effect(move || {
            let _ = ws.session.presence.read();
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
        moonkale_ext_api::crash::use_effect(move || {
            let _ = doc.read().text.len();
            let _ = ws.sources.graph_epoch.read();
            if !ready() {
                return;
            }
            let epoch = wiki_epoch.peek().wrapping_add(1);
            wiki_epoch.set(epoch);
            moonkale_ext_api::crash::spawn(async move {
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
        moonkale_ext_api::crash::use_effect(move || {
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
    // Save from any path (Ctrl+S, the menu, the toolbar): write through the
    // source and tell the language server, whose checks run on save.
    let save_now = {
        let uri = lsp_ident.as_ref().map(|(_, _, u)| u.clone());
        Callback::new(move |_: ()| {
            let uri = uri.clone();
            moonkale_ext_api::crash::spawn(async move {
                match ws.save(node).await {
                    Ok(()) => {
                        last_error.set(None);
                        if let Some(uri) = uri {
                            let document = doc.peek();
                            lsp.update_document(&uri, &document.text, &document.saved);
                        }
                    }
                    Err(e) => last_error.set(Some(e)),
                }
            });
        })
    };

    // Application commands aimed at the active editor (menus, keybindings).
    // Ignore commands sent before this view mounted, including remounts.
    let mut seen_command = use_signal(|| ws.shell.commands.peek().0);
    moonkale_ext_api::crash::use_effect(move || {
        let (seq, cmd) = *ws.shell.commands.read();
        if seq <= *seen_command.peek() {
            return;
        }
        seen_command.set(seq);

        if ws.docs.active.peek().as_ref() != Some(&node) {
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
            Some(Command::Editor(action)) => {
                if let Some(b) = backend.peek().as_ref() {
                    b.run(action);
                }
            }
            _ => {}
        }
    });

    // Every editor stages the same revision-checked, undoable unsaved edits.
    let apply_edit = Callback::new(
        move |(edit, base, connection): (
            moonkale_lsp::WorkspaceEdit,
            String,
            moonkale_lsp::LspSession,
        )| {
            let origin = doc.peek().node.clone();
            moonkale_ext_api::crash::spawn(async move {
                let uri = origin
                    .source
                    .as_str()
                    .strip_prefix("folder:")
                    .map(|root| crate::uri::file_uri(root, &origin.native_key));
                let current = || {
                    doc.peek().text == base
                        && *ws.docs.active.peek() == Some(node)
                        && uri.as_ref().is_some_and(|uri| {
                            lsp.peek_document_session(uri)
                                .is_some_and(|session| session.same_connection(&connection))
                        })
                };
                match crate::native_workspace_edit::apply_guarded(ws, &origin, lsp, &edit, current)
                    .await
                {
                    Ok(n) => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-applied", n = n));
                    }
                    Err(error) => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-edit-failed", error = error));
                    }
                }
            });
        },
    );
    let rename_ident = lsp_ident.clone();
    let mut rename_value = use_signal(String::new);
    let commit_rename = Callback::new(move |_: ()| {
        if *ws.docs.active.peek() != Some(node) {
            return;
        }
        let Some((line, col, _)) = rename_prompt.peek().clone() else {
            return;
        };
        let new_name = rename_value.peek().trim().to_string();
        rename_prompt.set(None);
        if new_name.is_empty() {
            return;
        }
        if let (Some(s), Some((_, _, uri))) = (lsp_session.peek().clone(), rename_ident.clone()) {
            let Some((base, connection)) = rename_base.peek().clone() else {
                return;
            };
            if doc.peek().text != base || !s.same_connection(&connection) {
                ws.set_status(t!(ws, L, "editor-rename-stale"));
                return;
            }
            moonkale_ext_api::crash::spawn(async move {
                match s.rename(&uri, line, col, &new_name).await {
                    Ok(edit) if edit.changes.is_empty() => {
                        let mut ws = ws;
                        ws.set_status(t!(ws, L, "editor-nothing-to-rename"));
                    }
                    Ok(edit) => apply_edit.call((edit, base, s)),
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
    let lang = d
        .node
        .language_hint()
        .map(str::to_string)
        .unwrap_or_else(|| t!(ws, L, "editor-plain-text"));
    let wrap_on = ws.settings.resolved.read().editor.wrap;
    let version = d.version;
    drop(d);

    let canonical_selection = if *ws.docs.active.read() == Some(node) {
        *ws.docs.selection.read()
    } else {
        None
    };
    rsx! {
            moonkale_ext_api::Stylesheet { href: PANEL_CSS }
            div {
                class: "mk-editor",
                "data-editor": "codemirror",
                "data-selection-anchor": canonical_selection.map(|(anchor, _)| anchor.to_string()),
                "data-selection-head": canonical_selection.map(|(_, head)| head.to_string()),
                // Ctrl+S / Cmd+S anywhere in the panel, including inside CodeMirror.
                onkeydown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::KeyboardEvent| {
                    let mods = e.modifiers();
                    if moonkale_ext_api::keys::primary(&mods) && e.key() == Key::Character("s".into()) {
                        e.prevent_default();
                        e.stop_propagation(); // the frame would dispatch Save again
                        save_now.call(());
                    }
                }),
                div { class: "mk-editor-toolbar",
                    span { class: "mk-editor-path", "{title}" }
                    if dirty { span { class: "mk-editor-dirty", title: t!(ws, L, "editor-unsaved"), "●" } }
                    span { class: "mk-editor-spacer" }
                    span { class: "mk-editor-meta", "{lang} · {version}" }
                    // Symbols, with their description on hover (ADR-0027).
                    moonkale_ext_api::icons::ToolbarActions {
                        primary: rsx! { moonkale_ext_api::icons::IconButton {
                        icon: "save",
                        label: t!(ws, L, "editor-save"),
                        command: "file.save".to_string(),
                        disabled: !dirty,
                        onclick: moonkale_ext_api::crash::event(save),
                    } },
    moonkale_ext_api::icons::IconButton {
                        icon: "wrap", command: "editor.toggleWrap".to_string(),
                        label: t!(ws, L, "editor-wrap"),
                        description: t!(ws, L, "editor-wrap-title"),
                        pressed: Some(wrap_on),
                        onclick: moonkale_ext_api::crash::event(move |_| toggle_wrap(ws)),
                    }

                    moonkale_ext_api::icons::IconButton {
                        icon: "reload",
                        label: t!(ws, L, "editor-reload"),
                        description: t!(ws, L, "editor-reload-title"),
                        onclick: moonkale_ext_api::crash::event(reload),
                    }
                    // Milestone 14: move this document to the Rust editor.
                    if ws.settings.resolved.read().extensions.is_enabled_id("dev.moonkale.editor-code-native", false) {
                        moonkale_ext_api::icons::IconButton {
                            icon: "swap",
                            label: "Rust".to_string(),
                            description: t!(ws, L, "editor-to-rust"),
                            class: "mk-editor-switch".to_string(),
                            onclick: moonkale_ext_api::crash::event(move |_| { let mut ws = ws; ws.choose_editor(node, "native"); }),
                        }
                    }
                    moonkale_ext_api::icons::PanelSettingsButton {}

                    }
                }
                if let Some((_, _, word)) = rename_prompt() {
                    div { class: "mk-editor-bar mk-editor-rename", role: "dialog", aria_label: t!(ws, L, "editor-rename"),
                        onkeydown: moonkale_ext_api::crash::event(move |e: KeyboardEvent| { if e.key() == Key::Escape { e.stop_propagation(); rename_prompt.set(None); } }),
                        span { {t!(ws, L, "editor-rename-prompt", word = word.clone())} }
                        input { id: "{rename_id}", class: "mk-input", value: "{rename_value}",
                            onmounted: moonkale_ext_api::crash::event({ let w = word.clone(); move |_| rename_value.set(w.clone()) }),
                            oninput: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| rename_value.set(e.value())),
                            onkeydown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::KeyboardEvent| {
                                e.stop_propagation();
                                if e.is_composing() { return; }
                                match e.key() {
                                    Key::Enter => { e.prevent_default(); commit_rename.call(()); }
                                    Key::Escape => { e.prevent_default(); rename_prompt.set(None); }
                                    _ => {}
                                }
                            }),
                        }
                        button { class: "mk-btn", onclick: moonkale_ext_api::crash::event(move |_| commit_rename.call(())), {t!(ws, L, "editor-rename")} }
                        button { class: "mk-btn", onclick: moonkale_ext_api::crash::event(move |_| rename_prompt.set(None)), {t!(ws, L, "editor-cancel")} }
                    }
                }
                if let Some(list) = actions() {
                    div { class: "mk-editor-bar mk-editor-actions", "data-count": "{list.len()}", role: "dialog", aria_label: t!(ws, L, "editor-actions"),
                        onkeydown: moonkale_ext_api::crash::event(move |e: KeyboardEvent| { if e.key() == Key::Escape { e.stop_propagation(); actions.set(None); } }),
                        if list.is_empty() { span { class: "mk-muted", {t!(ws, L, "editor-no-actions")} } } else { span { {t!(ws, L, "editor-actions")} } }
                        for (i, a) in list.iter().enumerate() {
                            button { key: "{i}", class: "mk-btn", title: "{a.kind}", onclick: moonkale_ext_api::crash::event({
                                let a = a.clone();
                                move |_| {
                                    actions.set(None);
                                    let a = a.clone();
                                    if let Some((base,s)) = actions_base.peek().clone().filter(|(base,_)|*base==doc.peek().text) {
                                        moonkale_ext_api::crash::spawn(async move {
                                            match s.resolve_code_action(&a).await {
                                                Ok(edit) => apply_edit.call((edit,base,s)),
                                                Err(e) => { let mut ws = ws; ws.set_status(format!("code action: {e}")); }
                                            }
                                        });
                                    }
                                }
                            }), "{a.title}" }
                        }
                        button { class: "mk-btn", onclick: moonkale_ext_api::crash::event(move |_| actions.set(None)), "✕" }
                    }
                }
                if let Some(list) = references() {
                    div { class: "mk-editor-bar mk-editor-refs", "data-count": "{list.len()}", role: "dialog", aria_label: t!(ws, L, "editor-references", n = list.len()),
                        onkeydown: moonkale_ext_api::crash::event(move |e: KeyboardEvent| { if e.key() == Key::Escape { e.stop_propagation(); references.set(None); } }),
                        span { {t!(ws, L, "editor-references", n = list.len())} }
                        button { class: "mk-btn", onclick: moonkale_ext_api::crash::event(move |_| references.set(None)), "✕" }
                        ul {
                            for (i, l) in list.iter().enumerate() {
                                li { key: "{i}", class: "mk-editor-ref", onclick: moonkale_ext_api::crash::event({
                                    let l = l.clone();
                                    let root = root_for_refs.clone();
                                    move |_| {
                                        let l = l.clone();
                                        let root = root.clone();
                                        moonkale_ext_api::crash::spawn(async move {
                                            let Some(root) = root else { return };
                                            if let Some(rel) = crate::uri::relative_file(&root,&l.uri) {
                                                if let Ok(n) = ws.open_relative_path(&rel).await {
                                                    let _ = ws.reveal(n, l.line, l.col).await;
                                                }
                                            }
                                        });
                                    }
                                }),
                                    { root_for_refs.as_ref().and_then(|r| crate::uri::relative_file(r,&l.uri)).unwrap_or_else(||l.uri.clone()) }
                                    span { class: "mk-muted", ":{l.line + 1}:{l.col + 1}" }
                                }
                            }
                        }
                    }
                }
                if let Some(err) = last_error() {
                    div { class: "mk-editor-error",
                        match err {
                            SourceError::Conflict { .. } => t!(ws, L, "editor-conflict"),
                            other => t!(ws, L, "editor-save-failed", error = other.to_string()),
                        }
                    }
                }
                div {
                    id: "{element_id}",
                    class: "mk-editor-host",
                    "data-selection-anchor": selection.map(|(anchor, _)| anchor.to_string()),
                    "data-selection-head": selection.map(|(_, head)| head.to_string()),
                    onmounted: moonkale_ext_api::crash::event(mount),
                    if let Some(err) = mount_error() {
                        div { class: "mk-editor-failed",
                            p { b { {t!(ws, L, "editor-failed")} } {t!(ws, L, "editor-failed-hint")} }
                            pre { "{err}" }
                        }
                    } else if !ready() {
                        div { class: "mk-editor-loading", {t!(ws, L, "editor-loading")} }
                    }
                }
            }
        }
}

/// Flip `editor.wrap` in the user settings (spec 014); every open editor
/// follows through its settings effect.
pub fn toggle_wrap(ws: Workspace) {
    let next = !ws.settings.resolved.peek().editor.wrap;
    moonkale_ext_api::crash::spawn_forever(async move {
        ws.update_user_settings(|f| f.editor.wrap = Some(next))
            .await;
    });
}
