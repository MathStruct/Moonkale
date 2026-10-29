//! Workspace host for the Rust-owned editor engine and virtualized view.
use crate::{
    native_language::{comments, language_for_hint},
    native_model::{utf16_position, NativeModel, Preferences},
    native_search::SearchHighlights,
    native_surface::RustEditorSurface,
    L,
};
use dioxus::prelude::*;
use dioxus_code::{advanced::CodeThemeStyles, CodeTheme, Theme};
use moonkale_core::NodeId;
use moonkale_ext_api::{editor::RevisionedDocument, t, Command, EditorAction, Workspace};

const PANEL_CSS: Asset = asset!("/assets/panel.css");
const NATIVE_CSS: Asset = asset!("/assets/native.css");

#[derive(Clone, Copy)]
struct NativeView {
    model: Signal<NativeModel>,
    first_row: Signal<usize>,
    first_column: Signal<usize>,
    scroll_displacement: Signal<f64>,
    horizontal_displacement: Signal<f64>,
    source_anchor: Signal<Option<crate::native_layout::ViewAnchor>>,
    run_heights: Signal<std::collections::BTreeMap<usize, crate::native_layout::CachedRunHeight>>,
    highlights: Signal<SearchHighlights>,
    seen_command: Signal<u64>,
    seen_reveal: Signal<u64>,
}

/// Opt-in Rust editor: Workspace owns text/history and editor-core owns input state.
#[component]
pub fn RustCodeEditorPanel(
    ws: Workspace,
    node: NodeId,
    #[props(default)] lsp: Option<crate::lsp::LspManager>,
) -> Element {
    // Hook state and async subscriptions belong to one document identity.
    // A host may reuse this public component while switching its node prop.
    // A key on a lone component does not trigger keyed-list reconciliation.
    // Keep even this single child in a keyed fragment so switching documents
    // remounts its hooks/effects instead of capturing the preceding model.
    rsx! {
        for scoped_node in [node] {
            RustCodeEditorView { key: "{scoped_node}", ws, node: scoped_node, lsp }
        }
    }
}

#[component]
fn RustCodeEditorView(ws: Workspace, node: NodeId, lsp: Option<crate::lsp::LspManager>) -> Element {
    let fallback = use_hook(move || crate::lsp::LspManager::for_workspace(ws));
    let manager = lsp.unwrap_or(fallback);
    let Some(mut doc) = ws.document(node) else {
        return rsx! {
            div { class: "mk-editor-failed", {t!(ws, L, "editor-document-closed")} }
        };
    };
    use_hook(move || {
        moonkale_ext_api::workspace::grouped_edits::register_view(ws, node, "native", None)
    });
    let diagnostics = crate::native_diagnostics::use_diagnostics(ws, node, manager);
    let (wiki_marks, follow_wiki) = crate::native_wiki::use_wiki(ws, node);
    let follow_link = crate::native_links::use_follow(ws, node);
    #[cfg(feature = "markdown-preview")]
    let follow_image = {
        let context = crate::native_images::ImageContext { ws, node };
        use_context_provider(move || context);
        crate::native_images::use_follow(context)
    };
    #[cfg(not(feature = "markdown-preview"))]
    let follow_image = Callback::new(|_: crate::native_image_parser::Image| {});
    let presence = use_memo(move || {
        let _ = ws.session.presence.read();
        let key = doc.read().node.native_key.clone();
        ws.others()
            .into_iter()
            .filter(|member| {
                member.active.as_deref() == Some(key.as_str()) && member.line.is_some()
            })
            .collect::<Vec<_>>()
    });
    let mut hover_target = use_signal(|| None::<(usize, usize)>);
    let wiki_hover = crate::native_wiki_hover::use_hover(ws, node, hover_target);
    use_context_provider(move || wiki_hover);
    let hover_text = crate::native_hover::use_hover(ws, node, manager, hover_target);
    let mut session = ws
        .editor_session(node)
        .expect("open document has a session");
    let language = language_for_hint(doc.peek().node.language_hint());
    let agent_panel =
        use_hook(move || moonkale_ext_api::links::panel_instance(format!("editor:{node}")));
    let agent_view = agent_panel.starts_with("lead:agent:");
    let create_view = |scope| NativeView {
        model: Signal::new_in_scope(NativeModel::new(session.peek().snapshot(), language), scope),
        first_row: Signal::new_in_scope(0, scope),
        first_column: Signal::new_in_scope(0, scope),
        scroll_displacement: Signal::new_in_scope(0.0, scope),
        horizontal_displacement: Signal::new_in_scope(0.0, scope),
        source_anchor: Signal::new_in_scope(None, scope),
        run_heights: Signal::new_in_scope(Default::default(), scope),
        highlights: Signal::new_in_scope(SearchHighlights::default(), scope),
        seen_command: Signal::new_in_scope(ws.shell.commands.peek().0, scope),
        seen_reveal: Signal::new_in_scope(0, scope),
    };
    // An agent range has its own model/camera. It must never move the person's
    // retained caret merely because both views show the same document.
    let isolated = use_hook(|| agent_view.then(|| create_view(dioxus::core::current_scope_id())));
    let view = isolated.unwrap_or_else(|| {
        *ws.editor_view_state_with_cleanup(
            node,
            || create_view(ScopeId::ROOT),
            |view: &NativeView| {
                view.model.manually_drop();
                view.first_row.manually_drop();
                view.first_column.manually_drop();
                view.scroll_displacement.manually_drop();
                view.horizontal_displacement.manually_drop();
                view.source_anchor.manually_drop();
                view.run_heights.manually_drop();
                view.highlights.manually_drop();
                view.seen_command.manually_drop();
                view.seen_reveal.manually_drop();
            },
        )
        .expect("open document has a view")
        .peek()
    });
    let NativeView {
        mut model,
        mut first_row,
        mut first_column,
        scroll_displacement,
        mut horizontal_displacement,
        source_anchor,
        run_heights,
        highlights,
        mut seen_command,
        mut seen_reveal,
    } = view;
    use_hook(move || {
        // Reconcile retained state synchronously at attachment. Mounted events
        // and effects may arrive after the first browser input event.
        if session.peek().snapshot().text != doc.peek().text {
            let canonical = doc.peek().text.clone();
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
        }
        if model.peek().revision != session.peek().snapshot().revision {
            model.with_mut(|state| state.replace(session.peek().snapshot()));
        }
        let settings = ws.settings.resolved.peek();
        model.with_mut(|state| {
            state.set_preferences(Preferences {
                wrap: settings.editor.wrap
                    || (cfg!(feature = "markdown-preview")
                        && settings.editor.markdown_preview
                        && language == Some(dioxus_code::Language::Markdown)),
                markdown_preview: settings.editor.markdown_preview,
                doc_comment_preview: settings.editor.doc_comment_preview,
                insert_spaces: settings.editor.insert_spaces,
                indent_width: settings.editor.indent_width,
            })
        });
        // An in-session backend change keeps the active canonical caret/range.
        // CodeMirror's report is UTF-16, including CRLF and non-BMP characters.
        if *ws.docs.active.peek() == Some(node) {
            if let Some((anchor, head)) = *ws.docs.selection.peek() {
                let revision = session.peek().snapshot().revision;
                session.with_mut(|state| {
                    state.set_selection(
                        revision,
                        moonkale_ext_api::editor::Utf16Selection { anchor, head },
                    );
                });
                model.with_mut(|state| state.restore_selection(session.peek().snapshot()));
            }
        }
    });
    moonkale_ext_api::crash::use_effect(move || {
        let settings = ws.settings.resolved.read();
        let preferences = Preferences {
            wrap: settings.editor.wrap
                || (cfg!(feature = "markdown-preview")
                    && settings.editor.markdown_preview
                    && language == Some(dioxus_code::Language::Markdown)),
            markdown_preview: settings.editor.markdown_preview,
            doc_comment_preview: settings.editor.doc_comment_preview,
            insert_spaces: settings.editor.insert_spaces,
            indent_width: settings.editor.indent_width,
        };
        drop(settings);
        if model.peek().preferences != preferences {
            model.with_mut(|state| state.set_preferences(preferences));
            let cursor = model.peek().engine.get_cursor_state().position;
            let row = model
                .peek()
                .engine
                .logical_position_to_visual(cursor.line, cursor.column)
                .map(|(row, _)| row)
                .unwrap_or(0);
            first_row.set(row.saturating_sub(3));
            first_column.set(0);
            horizontal_displacement.set(0.0);
        }
    });
    let mut focus_request = use_signal(|| 0u64);
    let mut search_mode = use_signal(|| None::<bool>);
    let mut last_error: Signal<Option<String>> = use_signal(|| None);
    let mut properties_draft = use_signal(|| None::<crate::native_properties::Draft>);
    let mut properties_error = use_signal(|| None::<String>);
    let mut properties_composing = use_signal(|| false);
    let properties = use_memo(move || {
        if !cfg!(feature = "markdown-preview")
            || language != Some(dioxus_code::Language::Markdown)
            || !ws.settings.resolved.read().editor.markdown_preview
        {
            return (false, String::new());
        }
        let base = session.read();
        crate::native_properties::front_matter(&base.snapshot().text)
            .map(|fm| (true, base.snapshot().text[fm.yaml].replace("\r\n", "\n")))
            .unwrap_or_default()
    });
    moonkale_ext_api::crash::use_effect(move || {
        if !ws.settings.resolved.read().editor.markdown_preview {
            properties_draft.set(None);
            properties_composing.set(false);
            properties_error.set(None);
        }
    });

    // External changes invalidate local history. The view also follows session
    // changes originating in another mounted panel, without replacing on every render.
    moonkale_ext_api::crash::use_effect(move || {
        let document = doc.read();
        if session.peek().snapshot().text != document.text {
            let canonical = document.text.clone();
            drop(document);
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
            let mut ws = ws;
            ws.clear_selection(node);
        }
    });
    moonkale_ext_api::crash::use_effect(move || {
        let revision = session.read().snapshot().revision;
        if model.peek().revision != revision {
            let snapshot = session.peek().snapshot().clone();
            model.with_mut(|state| state.replace(&snapshot));
            if let Some(selection) = snapshot.selection.filter(|_| !agent_view) {
                let mut ws = ws;
                ws.set_selection(node, selection.anchor, selection.head);
                if let Some((line, col)) =
                    crate::edit::utf16_offset_to_line_col(&snapshot.text, selection.head as usize)
                {
                    ws.set_cursor(node, line, col);
                }
            }
            publish_selection(ws, node, model, session);
        }
    });

    let text_revision = use_memo(move || session.read().snapshot().revision);
    moonkale_ext_api::crash::use_resource(move || {
        let revision = text_revision();
        async move {
            futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;
            if model
                .try_peek()
                .is_ok_and(|state| state.revision == revision && state.structure_dirty)
            {
                model.with_mut(|state| state.flush_structure());
            }
        }
    });

    let prepare_input = Callback::new(move |_: ()| {
        // Document writers may replace text before an effect gets scheduled.
        // Borrowed comparisons allocate nothing; clone only for actual divergence.
        if session.peek().snapshot().text != doc.peek().text {
            let canonical = doc.peek().text.clone();
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
        }
        if model.peek().revision != session.peek().snapshot().revision {
            model.with_mut(|state| state.replace(session.peek().snapshot()));
        }
    });
    // Returning to a retained tab must restore the workspace caret/word even
    // when the engine has not moved and no new input event will arrive.
    moonkale_ext_api::crash::use_effect(move || {
        if *ws.docs.active.read() == Some(node) {
            prepare_input.call(());
            publish_selection(ws, node, model, session);
        }
    });

    let changed = Callback::new(move |_: ()| {
        let delta = model.with_mut(|state| {
            let delta = state.engine.take_last_text_delta();
            state.engine.discard_undo_history();
            delta
        });
        if delta.is_none() {
            model.with_mut(|state| state.reveal_cursor());
            publish_selection(ws, node, model, session);
            return;
        }
        let base_state = session.peek();
        let base = base_state.snapshot();
        if model.peek().revision != base.revision || doc.peek().text != base.text {
            drop(base_state);
            let canonical = doc.peek().text.clone();
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
            model.with_mut(|state| state.replace(session.peek().snapshot()));
            let mut ws = ws;
            ws.clear_selection(node);
            return;
        }
        if let Some(delta) = delta {
            let result = model.peek().delta_batch(base, &delta);
            drop(base_state);
            match result {
                Ok(mut batch) => {
                    batch.selection = None;
                    match session.with_mut(|state| state.apply_in_place(&batch)) {
                        Ok(_) => {
                            let snapshot = session.peek();
                            model.with_mut(|state| {
                                state.update_highlight_deferred(snapshot.snapshot(), &batch)
                            });
                            doc.write().text = snapshot.snapshot().text.clone();
                        }
                        Err(error) => {
                            tracing::warn!(?error, "Rust editor rejected an engine delta");
                            model.with_mut(|state| state.replace(session.peek().snapshot()));
                        }
                    }
                }
                Err(reason) => {
                    tracing::warn!(reason, "Rust editor delta conversion failed");
                    model.with_mut(|state| state.replace(session.peek().snapshot()));
                }
            }
        }
        model.with_mut(|state| state.reveal_cursor());
        publish_selection(ws, node, model, session);
    });

    let (completion_menu, complete, accept_completion, close_completion) =
        crate::native_completion::use_completion(
            ws,
            node,
            manager,
            model,
            first_row,
            focus_request,
            changed,
        );
    let (definition, cancel_definition) =
        crate::native_definition::use_definition(ws, node, manager, model);
    let rename = crate::native_rename::use_rename(ws, node, manager, model, focus_request);
    let tools = crate::native_lsp_tools::use_tools(ws, node, manager, model, focus_request);
    let mut completion_selected = use_signal(|| 0usize);
    moonkale_ext_api::crash::use_effect(move || {
        let _ = completion_menu.read();
        completion_selected.set(0);
    });

    let toggle_comment = Callback::new(move |_: ()| {
        let config = comments(model.peek().language);
        if let Some(config) = config {
            model.with_mut(|state| {
                let _ = state.engine.execute(editor_core::Command::Edit(
                    editor_core::EditCommand::ToggleComment { config },
                ));
            });
            changed.call(());
        }
    });

    let history = Callback::new(move |undo: bool| {
        if let Some(group) = moonkale_ext_api::workspace::grouped_edits::next_group(ws, node, !undo)
        {
            crate::group_history::restore(ws, group, !undo);
            return;
        }
        let canonical = doc.peek().text.clone();
        if session.peek().snapshot().text != canonical {
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
            model.with_mut(|state| state.replace(session.peek().snapshot()));
            return;
        }
        let next = session.with_mut(|state| if undo { state.undo() } else { state.redo() });
        if let Ok(Some(snapshot)) = next {
            doc.write().text = snapshot.text.clone();
            model.with_mut(|state| state.replace(&snapshot));
            publish_selection(ws, node, model, session);
        }
    });
    let fold = Callback::new(move |(line, collapsed): (Option<usize>, bool)| {
        model.with_mut(|state| state.set_fold(line, collapsed));
        let total = model.peek().engine.total_visual_lines();
        let row = (*first_row.peek()).min(total.saturating_sub(1));
        first_row.set(row);
        publish_selection(ws, node, model, session);
    });
    let commit_properties = Callback::new(move |_: ()| {
        if properties_composing() {
            return false;
        }
        prepare_input.call(());
        let batch = match properties_draft.peek().as_ref() {
            None => return true,
            Some(draft) => draft.batch(session.peek().snapshot()),
        };
        match batch {
            Ok(Some(batch)) => {
                if session
                    .with_mut(|state| state.apply_in_place(&batch))
                    .is_err()
                {
                    properties_error.set(Some(t!(ws, L, "editor-properties-stale")));
                    return false;
                }
                let state = session.peek();
                doc.write().text = state.snapshot().text.clone();
                model.with_mut(|model| model.replace(state.snapshot()));
                drop(state);
                publish_selection(ws, node, model, session);
                wiki_hover.dismiss();
            }
            Ok(None) => {}
            Err(key) => {
                properties_error.set(Some(if key == "editor-properties-stale" {
                    t!(ws, L, "editor-properties-stale")
                } else {
                    t!(ws, L, "editor-properties-invalid")
                }));
                return false;
            }
        }
        properties_draft.set(None);
        properties_composing.set(false);
        properties_error.set(None);
        true
    });
    let save = Callback::new(move |_: ()| {
        if !commit_properties.call(()) {
            return;
        }
        moonkale_ext_api::crash::spawn(async move {
            match ws.save(node).await {
                Ok(()) => last_error.set(None),
                Err(error) => last_error.set(Some(error.to_string())),
            }
        });
    });
    let reload = Callback::new(move |_: ()| {
        properties_draft.set(None);
        properties_composing.set(false);
        properties_error.set(None);
        moonkale_ext_api::crash::spawn(async move {
            match ws.reload(node).await {
                Ok(()) => last_error.set(None),
                Err(error) => last_error.set(Some(error.to_string())),
            }
        });
    });
    use_hook(move || seen_command.set(ws.shell.commands.peek().0));
    moonkale_ext_api::crash::use_effect(move || {
        let (sequence, command) = *ws.shell.commands.read();
        if sequence <= *seen_command.peek() {
            return;
        }
        seen_command.set(sequence);
        if *ws.docs.active.peek() != Some(node) {
            return;
        }
        match command {
            Some(Command::Editor(EditorAction::Find)) => search_mode.set(Some(false)),
            Some(Command::Editor(EditorAction::Replace)) => search_mode.set(Some(true)),
            Some(Command::Editor(EditorAction::ToggleComment)) => toggle_comment.call(()),
            Some(Command::Editor(EditorAction::FoldAll)) => fold.call((None, true)),
            Some(Command::Editor(EditorAction::UnfoldAll)) => fold.call((None, false)),
            Some(Command::Editor(EditorAction::NativeReload)) => reload.call(()),
            Some(Command::Editor(EditorAction::NativeHover)) => {
                let caret = model.peek().engine.get_cursor_state().position;
                hover_target.set(Some((caret.line, caret.column)));
            }
            Some(Command::Editor(EditorAction::NativeComplete)) => {
                hover_target.set(None);
                complete.call(());
                focus_request.with_mut(|value| *value += 1);
            }
            Some(Command::Editor(EditorAction::Definition)) => {
                hover_target.set(None);
                close_completion.call(());
                tools.dismiss.call(());
                rename.dismiss.call(());
                definition.call(());
            }
            Some(Command::Editor(EditorAction::Rename)) => {
                hover_target.set(None);
                close_completion.call(());
                cancel_definition.call(());
                tools.dismiss.call(());
                rename.invoke.call(());
            }
            Some(Command::Editor(EditorAction::CodeActions | EditorAction::References)) => {
                hover_target.set(None);
                close_completion.call(());
                cancel_definition.call(());
                rename.dismiss.call(());
                if matches!(command, Some(Command::Editor(EditorAction::CodeActions))) {
                    tools.actions.call(());
                } else {
                    tools.references.call(());
                }
            }
            Some(Command::Editor(EditorAction::NativePreview))
                if cfg!(feature = "markdown-preview")
                    && model.peek().language == Some(dioxus_code::Language::Markdown) =>
            {
                let next = !ws.settings.resolved.peek().editor.markdown_preview;
                moonkale_ext_api::crash::spawn(async move {
                    ws.update_user_settings(|file| file.editor.markdown_preview = Some(next))
                        .await;
                });
            }
            Some(Command::Editor(EditorAction::NativeDocPreview))
                if cfg!(feature = "markdown-preview")
                    && matches!(
                        model.peek().language.map(|language| language.slug()),
                        Some("rust" | "python" | "julia")
                    ) =>
            {
                let next = !ws.settings.resolved.peek().editor.doc_comment_preview;
                moonkale_ext_api::crash::spawn(async move {
                    ws.update_user_settings(|file| file.editor.doc_comment_preview = Some(next))
                        .await;
                });
            }
            Some(Command::Editor(EditorAction::NativeCodeMirror))
                if ws
                    .settings
                    .resolved
                    .peek()
                    .extensions
                    .is_enabled_id("dev.moonkale.editor-code", true) =>
            {
                let mut ws = ws;
                ws.choose_editor(node, "codemirror");
            }
            Some(Command::Save) => save.call(()),
            Some(Command::Undo) => history.call(true),
            Some(Command::Redo) => history.call(false),
            _ => {}
        }
    });

    let mut seen_agent_range = use_signal(|| None);
    moonkale_ext_api::crash::use_effect(move || {
        let range = ws.links.reveals.read().get(&agent_panel).cloned();
        if range == *seen_agent_range.peek() {
            return;
        }
        if let Some(range) = &range {
            let text = doc.peek().text.clone();
            let position = |line: u32, col: u32| {
                let start = text
                    .split_inclusive('\n')
                    .take(line as usize)
                    .map(|s| s.encode_utf16().count() as u32)
                    .sum::<u32>();
                let len = text
                    .split('\n')
                    .nth(line as usize)
                    .unwrap_or_default()
                    .encode_utf16()
                    .count() as u32;
                utf16_position(&text, start.saturating_add(col.min(len)))
            };
            let start = position(range.line, range.column);
            let end = position(
                range.end_line.unwrap_or(range.line),
                range.end_column.unwrap_or(range.column),
            );
            model.with_mut(|state| {
                let _ = state.engine.execute(editor_core::Command::Cursor(
                    editor_core::CursorCommand::SetSelection { start, end },
                ));
                state.reveal_cursor();
            });
            first_row.set(start.line.saturating_sub(3));
            seen_agent_range.set(Some(range.clone()));
        }
    });
    moonkale_ext_api::crash::use_effect(move || {
        let Some(reveal) = *ws.docs.reveal.read() else {
            return;
        };
        if agent_view || reveal.node != node || reveal.seq <= *seen_reveal.peek() {
            return;
        }
        seen_reveal.set(reveal.seq);
        prepare_input.call(());
        let text = doc.peek().text.clone();
        let line_start = text
            .split_inclusive('\n')
            .take(reveal.line as usize)
            .map(|line| line.encode_utf16().count() as u32)
            .sum::<u32>();
        let line_length = text
            .split('\n')
            .nth(reveal.line as usize)
            .unwrap_or_default()
            .trim_end_matches('\r')
            .encode_utf16()
            .count() as u32;
        let position = utf16_position(
            &text,
            line_start.saturating_add(reveal.col.min(line_length)),
        );
        let visual_row = model.with_mut(|state| {
            crate::editor_core_spike::place_caret_at(&mut state.engine, position);
            state.reveal_cursor();
            state
                .engine
                .editor()
                .logical_position_to_visual(position.line, position.column)
                .map(|(row, _)| row)
                .unwrap_or(0)
        });
        first_row.set(visual_row.saturating_sub(3));
        publish_selection(ws, node, model, session);
        focus_request.with_mut(|value| *value += 1);
    });

    let document = doc.read();
    let title = document.node.native_key.clone();
    let dirty = document.dirty();
    let version = document.version.0;
    let language_label = document
        .node
        .language_hint()
        .map(str::to_owned)
        .unwrap_or_else(|| t!(ws, L, "editor-plain-text"));
    drop(document);
    let theme = CodeTheme::fixed(if *ws.shell.theme_light.read() {
        Theme::GITHUB_LIGHT
    } else {
        Theme::TOKYO_NIGHT
    });
    let selection = if *ws.docs.active.read() == Some(node) {
        *ws.docs.selection.read()
    } else {
        None
    };
    let codemirror_on = ws
        .settings
        .resolved
        .read()
        .extensions
        .is_enabled_id("dev.moonkale.editor-code", true);
    let cursor_word = (*ws.docs.active.read() == Some(node))
        .then(|| model.read().cursor_word_preview(128))
        .flatten();
    let editor_settings = ws.settings.resolved.read().editor.clone();
    let preview_on = cfg!(feature = "markdown-preview")
        && editor_settings.markdown_preview
        && language == Some(dioxus_code::Language::Markdown);
    let wrap_on = editor_settings.wrap || preview_on;
    let actions_label =
        moonkale_ext_api::command_hint("editor.codeActions", t!(ws, L, "editor-actions-shortcut"))
            .title;
    rsx! {
        div {
            class: "mk-editor mk-crust",
            "data-editor": "rust",
            "data-selection-anchor": selection.map(|(anchor, _)| anchor.to_string()),
            "data-selection-head": selection.map(|(_, head)| head.to_string()),
            onkeydown: moonkale_ext_api::crash::event(move |event: dioxus::prelude::KeyboardEvent| {
                if moonkale_ext_api::keys::primary(&event.modifiers()) {
                    if let Key::Character(key) = event.key() {
                        if key.eq_ignore_ascii_case("f") || key.eq_ignore_ascii_case("h") {
                            event.prevent_default();
                            event.stop_propagation();
                            search_mode.set(Some(key.eq_ignore_ascii_case("h")));
                        }
                        if key.eq_ignore_ascii_case("s") {
                            event.prevent_default();
                            event.stop_propagation();
                            save.call(());
                        }
                    }
                }
            }),
            document::Stylesheet { href: PANEL_CSS }
            document::Stylesheet { href: NATIVE_CSS }
            CodeThemeStyles { theme }
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{title}" }
                if dirty {
                    span {
                        class: "mk-editor-dirty",
                        title: t!(ws, L, "editor-unsaved"),
                        "●"
                    }
                }
                span { class: "mk-editor-cursor-word", aria_hidden: cursor_word.is_none().to_string(),
                    if let Some(word) = cursor_word.as_ref() { "‹{word}›" }
                }
                span { class: "mk-editor-spacer" }
                span { class: "mk-editor-meta",
                    {
                        t!(
                            ws, L, "editor-rust-meta", language = language_label, version = version
                            .to_string()
                        )
                    }
                }
                moonkale_ext_api::icons::ToolbarActions {
                    primary: rsx! { moonkale_ext_api::icons::IconButton {
                    icon: "save", label: t!(ws, L, "editor-save"),
                    command: "file.save".to_string(),
                    disabled: !dirty && properties_draft.read().is_none(),
                    onclick: moonkale_ext_api::crash::event(move |_| save.call(())),
                } },

                moonkale_ext_api::icons::IconButton {
                    icon: "reload", command: "native.reload".to_string(), label: t!(ws, L, "editor-reload"),
                    description: t!(ws, L, "editor-reload-title"),
                    onclick: moonkale_ext_api::crash::event(move |_| reload.call(())),
                }
                moonkale_ext_api::icons::IconButton {
                    icon: "wrap", command: "editor.toggleWrap".to_string(), label: t!(ws, L, "editor-wrap"),
                    description: t!(ws, L, "editor-wrap-title"),
                    pressed: Some(wrap_on), disabled: preview_on,
                    class: "mk-native-wrap".to_string(),
                    onclick: moonkale_ext_api::crash::event(move |_| crate::panel::toggle_wrap(ws)),
                }
                                moonkale_ext_api::icons::PanelSettingsButton {}
                if cfg!(feature = "markdown-preview") && language == Some(dioxus_code::Language::Markdown) {
                    button {
                        class: if preview_on { "mk-btn mk-btn-on mk-native-preview" } else { "mk-btn mk-native-preview" },
                        aria_pressed: preview_on.to_string(),

                         title: moonkale_ext_api::command_hint("native.preview", t!(ws, L, "editor-preview-title")).title, "data-command": "native.preview", "data-shortcut": moonkale_ext_api::command_hint("native.preview", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| {
                            let next = !ws.settings.resolved.peek().editor.markdown_preview;
                            moonkale_ext_api::crash::spawn_forever(async move {
                                ws.update_user_settings(|file| file.editor.markdown_preview = Some(next)).await;
                            });
                            focus_request.with_mut(|value| *value += 1);
                        }),
                        {t!(ws, L, "editor-preview")}
                    }
                }
                if cfg!(feature = "markdown-preview") && matches!(language.map(|language| language.slug()), Some("rust" | "python" | "julia")) {
                    button { class: if editor_settings.doc_comment_preview { "mk-btn mk-btn-on mk-native-doc-preview" } else { "mk-btn mk-native-doc-preview" },
                        aria_pressed: editor_settings.doc_comment_preview.to_string(),
                         title: moonkale_ext_api::command_hint("native.docPreview", t!(ws, L, "editor-doc-preview-title")).title, "data-command": "native.docPreview", "data-shortcut": moonkale_ext_api::command_hint("native.docPreview", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| {
                            let next = !ws.settings.resolved.peek().editor.doc_comment_preview;
                            moonkale_ext_api::crash::spawn(async move { ws.update_user_settings(|file| file.editor.doc_comment_preview = Some(next)).await; });
                            focus_request.with_mut(|value| *value += 1);
                        }), {t!(ws, L, "editor-doc-preview")}
                    }
                }
                button {
                    class: "mk-btn mk-native-show-hover",

                     title: moonkale_ext_api::command_hint("native.hover", t!(ws, L, "editor-hover")).title, "data-command": "native.hover", "data-shortcut": moonkale_ext_api::command_hint("native.hover", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| {
                        let caret = model.peek().engine.get_cursor_state().position;
                        hover_target.set(Some((caret.line, caret.column)));
                    }),
                    {t!(ws, L, "editor-hover")}
                }
                button { class: "mk-btn mk-native-complete",   title: moonkale_ext_api::command_hint("native.complete", t!(ws, L, "editor-completion")).title, "data-command": "native.complete", "data-shortcut": moonkale_ext_api::command_hint("native.complete", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| { hover_target.set(None); complete.call(()); focus_request.with_mut(|value| *value += 1); }), {moonkale_ext_api::command_hint("native.complete", t!(ws, L, "editor-completion")).title} }
                button { class: "mk-btn mk-native-definition",   title: moonkale_ext_api::command_hint("editor.definition", t!(ws, L, "editor-definition")).title, "data-command": "editor.definition", "data-shortcut": moonkale_ext_api::command_hint("editor.definition", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| { hover_target.set(None); close_completion.call(()); tools.dismiss.call(()); rename.dismiss.call(()); definition.call(()); }), {t!(ws, L, "editor-definition")} }
                button { class: "mk-btn mk-native-rename",   title: moonkale_ext_api::command_hint("editor.rename", t!(ws, L, "editor-rename-shortcut")).title, "data-command": "editor.rename", "data-shortcut": moonkale_ext_api::command_hint("editor.rename", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| { hover_target.set(None); close_completion.call(()); cancel_definition.call(()); tools.dismiss.call(()); rename.invoke.call(()); }), {moonkale_ext_api::command_hint("editor.rename", t!(ws, L, "editor-rename-shortcut")).title} }
                button { class:"mk-btn mk-native-code-actions",   title: moonkale_ext_api::command_hint("editor.codeActions", t!(ws, L, "editor-actions-shortcut")).title, "data-command": "editor.codeActions", "data-shortcut": moonkale_ext_api::command_hint("editor.codeActions", "").shortcut, onclick:moonkale_ext_api::crash::event(move |_| { hover_target.set(None); close_completion.call(()); cancel_definition.call(()); rename.dismiss.call(()); tools.actions.call(()); }), "{actions_label}" }
                button { class:"mk-btn mk-native-find-references",   title: moonkale_ext_api::command_hint("editor.references", t!(ws, L, "editor-references-shortcut")).title, "data-command": "editor.references", "data-shortcut": moonkale_ext_api::command_hint("editor.references", "").shortcut, onclick:moonkale_ext_api::crash::event(move |_| { hover_target.set(None); close_completion.call(()); cancel_definition.call(()); rename.dismiss.call(()); tools.references.call(()); }), {moonkale_ext_api::command_hint("editor.references", t!(ws,L,"editor-references-shortcut")).title} }
                if comments(model.read().language).is_some() {
                    button { class: "mk-btn mk-native-comment",

                         title: moonkale_ext_api::command_hint("editor.toggleComment", t!(ws, L, "editor-toggle-comment")).title, "data-command": "editor.toggleComment", "data-shortcut": moonkale_ext_api::command_hint("editor.toggleComment", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| { toggle_comment.call(()); focus_request.with_mut(|value| *value += 1); }),
                        {moonkale_ext_api::command_hint("editor.toggleComment", t!(ws, L, "editor-toggle-comment")).title}
                    }
                }
                if !model.read().structure.folds.is_empty() {
                    button { class: "mk-btn mk-native-fold-all",  title: moonkale_ext_api::command_hint("editor.foldAll", t!(ws, L, "editor-fold-all")).title, "data-command": "editor.foldAll", "data-shortcut": moonkale_ext_api::command_hint("editor.foldAll", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| { fold.call((None, true)); focus_request.with_mut(|value| *value += 1); }), {t!(ws, L, "editor-fold-all")} }
                    button { class: "mk-btn mk-native-unfold-all",  title: moonkale_ext_api::command_hint("editor.unfoldAll", t!(ws, L, "editor-unfold-all")).title, "data-command": "editor.unfoldAll", "data-shortcut": moonkale_ext_api::command_hint("editor.unfoldAll", "").shortcut, onclick: moonkale_ext_api::crash::event(move |_| { fold.call((None, false)); focus_request.with_mut(|value| *value += 1); }), {t!(ws, L, "editor-unfold-all")} }
                }
                if codemirror_on {
                    button {
                        class: "mk-btn mk-editor-switch",
                        title: t!(ws, L, "editor-to-codemirror"),
                        onclick: moonkale_ext_api::crash::event(move |_| {
                            let mut ws = ws;
                            ws.choose_editor(node, "codemirror");
                        }),
                        "CodeMirror"
                    }
                }


                }
            }
            if preview_on {
                crate::native_properties::Properties {
                    ws, yaml: properties.read().1.clone(), has: properties.read().0,
                    draft: properties_draft, composing: properties_composing, error: properties_error(),
                    onopen: move |_| {
                        prepare_input.call(());
                        properties_draft.set(Some(crate::native_properties::Draft::new(session.peek().snapshot())));
                        properties_composing.set(false);
                        properties_error.set(None);
                        wiki_hover.dismiss();
                    },
                    onapply: move |_| { if commit_properties.call(()) { focus_request.with_mut(|value| *value += 1); } },
                    oncancel: move |_| { properties_draft.set(None); properties_composing.set(false); properties_error.set(None); focus_request.with_mut(|value| *value += 1); },
                    onsave: save,
                }
            }
            crate::native_rename::RenamePrompt { ws, rename }
            crate::native_lsp_tools::ToolsMenu { ws, node, tools }
            if let Some(error) = last_error() {
                div { class: "mk-editor-bar mk-editor-error", "{error}" }
            }
            if let Some(replacing) = search_mode() {
                crate::native_search::NativeSearch { ws, model, first_row, highlights, onchange: moonkale_ext_api::crash::event(move |value| changed.call(value)), replacing, onclose: move |_| { search_mode.set(None); focus_request.with_mut(|value| *value += 1); } }
            }
            if !diagnostics.read().is_empty() {
                div { class: "mk-native-diagnostics", role: "region", aria_label: t!(ws, L, "editor-diagnostics"),
                    for diagnostic in diagnostics.read().iter() {
                        button {
                            class: "mk-native-diagnostic-message mk-native-diagnostic-{diagnostic.severity}",
                            onclick: moonkale_ext_api::crash::event({
                                let line = diagnostic.line;
                                let col = diagnostic.col;
                                move |_| {
                                    let target = doc.peek().node.clone();
                                    focus_request.with_mut(|value| *value += 1);
                                    moonkale_ext_api::crash::spawn(async move { let _ = ws.reveal(target, line, col).await; });
                                }
                            }),
                            "{diagnostic.line + 1}:{diagnostic.col + 1} — {diagnostic.message}"
                        }
                    }
                }
            }
            RustEditorSurface {
                onactions: tools.actions,
                onreferences: tools.references,
                oncancel_tools: tools.cancel,
                ondismiss_tools: tools.dismiss,
                ondismiss_rename: rename.dismiss,
                onrename: rename.invoke,
                oncancel_rename: rename.cancel,
                ondefinition: definition,
                oncancel_definition: cancel_definition,
                completion_items: completion_menu().map(|menu| menu.items).unwrap_or_default(),
                completion_selected,
                oncompletion: complete,
                oncompletion_accept: accept_completion,
                oncompletion_close: close_completion,
                completion_label: t!(ws, L, "editor-completion"),
                wiki_enabled: doc.read().node.language_hint() == Some("markdown"),
                hover_target,
                hover_text: hover_text(),
                hover_label: t!(ws, L, "editor-hover"),
                wiki_popup: Some(rsx!{crate::native_wiki_hover::WikiPopup{control:wiki_hover,ws,onclose:move |_|focus_request.with_mut(|value|*value+=1)}}),
                diagnostics,
                presence: presence(),
                wiki_marks: wiki_marks(),
                onwiki: follow_wiki,
                onlink: follow_link,
                onimage: follow_image,
                model,
                highlights,
                first_row,
                first_column,
                scroll_displacement,
                horizontal_displacement,
                source_anchor,
                run_heights,
                onchange: moonkale_ext_api::crash::event(move |value| changed.call(value)),
                onprepare: prepare_input,
                onhistory: history,
                focus_request,
                onfold: move |(line, collapsed)| fold.call((Some(line), collapsed)),
                fold_label: t!(ws, L, "editor-fold"),
                unfold_label: t!(ws, L, "editor-unfold"),
                label: format!("{} — {}", t!(ws, L, "editor-rust-label"), title),
                accessibility_help: t!(ws, L, "editor-a11y-help", primary = moonkale_ext_api::keys::primary_name()),
                accessibility_position: move |(line, column)| t!(ws, L, "editor-a11y-position", line = line, column = column),
                accessibility_selection: move |(line, column, end_line, end_column)| t!(ws, L, "editor-a11y-selection", line = line, column = column, end_line = end_line, end_column = end_column),
                accessibility_tab_mode: move |moves: bool| if moves { t!(ws, L, "editor-a11y-tab-focus") } else { t!(ws, L, "editor-a11y-tab-indent") },
                theme_class: theme.classes(),
            }
        }
    }
}
fn publish_selection(
    mut ws: Workspace,
    node: NodeId,
    model: Signal<NativeModel>,
    mut session: Signal<RevisionedDocument>,
) {
    let snapshot = session.peek();
    let selection = model.peek().selection(&snapshot.snapshot().text);
    let revision = snapshot.snapshot().revision;
    let (line, column) = model.peek().cursor_line_col(&snapshot.snapshot().text);
    drop(snapshot);
    if session.peek().snapshot().selection != Some(selection) {
        session.with_mut(|state| state.set_selection(revision, selection));
    }
    // Workspace state also needs publication when switching between retained tabs.
    ws.set_editor_selection(node, revision, selection.anchor, selection.head);
    ws.set_cursor(node, line, column);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_hints_select_enabled_grammars_and_unknowns_are_plain_text() {
        assert!(language_for_hint(Some("rust")).is_some());
        assert!(language_for_hint(Some("julia")).is_some());
        assert!(language_for_hint(Some("lean")).is_some());
        assert!(language_for_hint(Some("nix")).is_some());
        assert!(language_for_hint(Some("plain-unknown")).is_none());
        assert!(language_for_hint(None).is_none());
    }
}
