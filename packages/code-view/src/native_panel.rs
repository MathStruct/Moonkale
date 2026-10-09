//! Workspace host for the Rust-owned editor engine and virtualized view.
use crate::{
    native_language::{comments, language_for_hint},
    native_model::{delta_batch_with_length, utf16_position, NativeModel, Preferences},
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
    rsx! { RustCodeEditorView { key: "{node}", ws, node, lsp } }
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
    let diagnostics = crate::native_diagnostics::use_diagnostics(ws, node, manager);
    let (wiki_marks, follow_wiki) = crate::native_wiki::use_wiki(ws, node);
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
    let hover_text = crate::native_hover::use_hover(ws, node, manager, hover_target);
    let mut session = ws
        .editor_session(node)
        .expect("open document has a session");
    let language = language_for_hint(doc.peek().node.language_hint());
    let view = ws
        .editor_view_state_with_cleanup(
            node,
            || NativeView {
                model: Signal::new_in_scope(
                    NativeModel::new(session.peek().snapshot(), language),
                    ScopeId::ROOT,
                ),
                first_row: Signal::new_in_scope(0, ScopeId::ROOT),
                first_column: Signal::new_in_scope(0, ScopeId::ROOT),
                scroll_displacement: Signal::new_in_scope(0.0, ScopeId::ROOT),
                source_anchor: Signal::new_in_scope(None, ScopeId::ROOT),
                run_heights: Signal::new_in_scope(Default::default(), ScopeId::ROOT),
                highlights: Signal::new_in_scope(SearchHighlights::default(), ScopeId::ROOT),
                seen_command: Signal::new_in_scope(ws.shell.commands.peek().0, ScopeId::ROOT),
                seen_reveal: Signal::new_in_scope(0, ScopeId::ROOT),
            },
            |view| {
                view.model.manually_drop();
                view.first_row.manually_drop();
                view.first_column.manually_drop();
                view.scroll_displacement.manually_drop();
                view.source_anchor.manually_drop();
                view.run_heights.manually_drop();
                view.highlights.manually_drop();
                view.seen_command.manually_drop();
                view.seen_reveal.manually_drop();
            },
        )
        .expect("open document has a view");
    let NativeView {
        mut model,
        mut first_row,
        mut first_column,
        scroll_displacement,
        source_anchor,
        run_heights,
        highlights,
        mut seen_command,
        mut seen_reveal,
    } = *view.peek();
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
                wrap: settings.editor.wrap,
                insert_spaces: settings.editor.insert_spaces,
                indent_width: settings.editor.indent_width,
            })
        });
    });
    use_effect(move || {
        let settings = ws.settings.resolved.read();
        let preferences = Preferences {
            wrap: settings.editor.wrap,
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
        }
    });
    let set_indentation = Callback::new(move |(spaces, width): (Option<bool>, Option<u8>)| {
        dioxus::core::spawn_forever(async move {
            ws.update_user_settings(move |file| {
                file.editor.insert_spaces = spaces;
                file.editor.indent_width = width;
            })
            .await;
        });
    });
    let mut focus_request = use_signal(|| 0u64);
    let mut search_mode = use_signal(|| None::<bool>);
    let mut last_error: Signal<Option<String>> = use_signal(|| None);

    // External changes invalidate local history. The view also follows session
    // changes originating in another mounted panel, without replacing on every render.
    use_effect(move || {
        let document = doc.read();
        if session.peek().snapshot().text != document.text {
            let canonical = document.text.clone();
            drop(document);
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
            let mut ws = ws;
            ws.clear_selection(node);
        }
    });
    use_effect(move || {
        let revision = session.read().snapshot().revision;
        if model.peek().revision != revision {
            let snapshot = session.peek().snapshot().clone();
            model.with_mut(|state| state.replace(&snapshot));
            if let Some(selection) = snapshot.selection {
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
    use_resource(move || {
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
    use_effect(move || {
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
        let base = session.peek().snapshot().clone();
        if model.peek().revision != base.revision || doc.peek().text != base.text {
            let canonical = doc.peek().text.clone();
            let _ = session.with_mut(|state| state.replace_from_workspace(canonical));
            model.with_mut(|state| state.replace(session.peek().snapshot()));
            let mut ws = ws;
            ws.clear_selection(node);
            return;
        }
        if let Some(delta) = delta {
            let result = delta_batch_with_length(
                &base,
                &delta,
                moonkale_ext_api::editor::Utf16Selection { anchor: 0, head: 0 },
                model.peek().normalized_chars,
            );
            match result {
                Ok(mut batch) => {
                    batch.selection = None;
                    match session.with_mut(|state| state.apply(&batch)) {
                        Ok(snapshot) => {
                            model.with_mut(|state| {
                                state.update_highlight_deferred(&snapshot, &batch)
                            });
                            doc.write().text = snapshot.text;
                        }
                        Err(error) => {
                            tracing::warn!(?error, "Rust editor rejected an engine delta");
                            model.with_mut(|state| state.replace(session.peek().snapshot()));
                        }
                    }
                }
                Err(reason) => {
                    tracing::warn!(reason, "Rust editor delta conversion failed");
                    model.with_mut(|state| state.replace(&base));
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
    use_effect(move || {
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
    let save = Callback::new(move |_: ()| {
        spawn(async move {
            match ws.save(node).await {
                Ok(()) => last_error.set(None),
                Err(error) => last_error.set(Some(error.to_string())),
            }
        });
    });
    let reload = Callback::new(move |_: ()| {
        spawn(async move {
            match ws.reload(node).await {
                Ok(()) => last_error.set(None),
                Err(error) => last_error.set(Some(error.to_string())),
            }
        });
    });
    use_hook(move || seen_command.set(ws.shell.commands.peek().0));
    use_effect(move || {
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
            Some(Command::Save) => save.call(()),
            Some(Command::Undo) => history.call(true),
            Some(Command::Redo) => history.call(false),
            Some(Command::CloseEditor) => ws.close_node(node),
            _ => {}
        }
    });
    use_effect(move || {
        let Some(reveal) = *ws.docs.reveal.read() else {
            return;
        };
        if reveal.node != node || reveal.seq <= *seen_reveal.peek() {
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
    let cursor_word = ws.cursor_word();
    let editor_settings = ws.settings.resolved.read().editor.clone();
    let wrap_on = editor_settings.wrap;
    let spaces_value = editor_settings
        .insert_spaces
        .map(|spaces| if spaces { "spaces" } else { "tabs" })
        .unwrap_or("default");
    let width_value = editor_settings
        .indent_width
        .map(|width| width.to_string())
        .unwrap_or_default();
    rsx! {
        div {
            class: "mk-editor mk-crust",
            "data-editor": "rust",
            "data-selection-anchor": selection.map(|(anchor, _)| anchor.to_string()),
            "data-selection-head": selection.map(|(_, head)| head.to_string()),
            onkeydown: move |event| {
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
            },
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
                span { class: "mk-editor-spacer" }
                span { class: "mk-editor-meta",
                    {
                        t!(
                            ws, L, "editor-rust-meta", language = language_label, version = version
                            .to_string()
                        )
                    }
                }
                button {
                    class: "mk-btn",
                    disabled: !dirty,
                    onclick: move |_| save.call(()),
                    {t!(ws, L, "editor-save")}
                }
                button {
                    class: "mk-btn",
                    onclick: move |_| reload.call(()),
                    title: t!(ws, L, "editor-reload-title"),
                    {t!(ws, L, "editor-reload")}
                }
                button {
                    class: if wrap_on { "mk-btn mk-btn-on mk-native-wrap" } else { "mk-btn mk-native-wrap" },
                    aria_pressed: wrap_on.to_string(),
                    title: t!(ws, L, "editor-wrap-title"),
                    onclick: move |_| crate::panel::toggle_wrap(ws),
                    {t!(ws, L, "editor-wrap")}
                }
                label { class: "mk-native-indent-control",
                    {t!(ws, L, "editor-indent-style")}
                    select { class: "mk-native-indent-style", aria_label: t!(ws, L, "editor-indent-style"), value: spaces_value,
                        onchange: move |event| {
                            let spaces = match event.value().as_str() { "spaces" => Some(true), "tabs" => Some(false), _ => None };
                            let width = ws.settings.user.peek().editor.indent_width;
                            set_indentation.call((spaces, width));
                        },
                        option { value: "default", selected: spaces_value == "default", {t!(ws, L, "editor-language-default")} }
                        option { value: "spaces", selected: spaces_value == "spaces", {t!(ws, L, "editor-indent-spaces")} }
                        option { value: "tabs", selected: spaces_value == "tabs", {t!(ws, L, "editor-indent-tabs")} }
                    }
                }
                label { class: "mk-native-indent-control",
                    {t!(ws, L, "editor-indent-width")}
                    select { class: "mk-native-indent-width", aria_label: t!(ws, L, "editor-indent-width"), value: width_value,
                        onchange: move |event| {
                            let width = event.value().parse::<u8>().ok();
                            let spaces = ws.settings.user.peek().editor.insert_spaces;
                            set_indentation.call((spaces, width));
                        },
                        option { value: "", selected: editor_settings.indent_width.is_none(), {t!(ws, L, "editor-language-default")} }
                        for width in 1..=8 { option { value: "{width}", selected: editor_settings.indent_width == Some(width), "{width}" } }
                    }
                }
                button {
                    class: "mk-btn mk-native-show-hover",
                    title: t!(ws, L, "editor-hover"),
                    onclick: move |_| {
                        let caret = model.peek().engine.get_cursor_state().position;
                        hover_target.set(Some((caret.line, caret.column)));
                    },
                    {t!(ws, L, "editor-hover")}
                }
                button { class: "mk-btn mk-native-complete", title: t!(ws, L, "editor-completion"), onclick: move |_| { hover_target.set(None); complete.call(()); focus_request.with_mut(|value| *value += 1); }, {t!(ws, L, "editor-completion")} }
                button { class: "mk-btn mk-native-definition", title: t!(ws, L, "editor-definition"), onclick: move |_| { hover_target.set(None); close_completion.call(()); tools.dismiss.call(()); rename.dismiss.call(()); definition.call(()); }, {t!(ws, L, "editor-definition")} }
                button { class: "mk-btn mk-native-rename", title: t!(ws, L, "editor-rename-shortcut"), onclick: move |_| { hover_target.set(None); close_completion.call(()); cancel_definition.call(()); tools.dismiss.call(()); rename.invoke.call(()); }, {t!(ws, L, "editor-rename")} }
                button { class:"mk-btn mk-native-code-actions", title:t!(ws,L,"editor-actions-shortcut"), onclick:move |_| { hover_target.set(None); close_completion.call(()); cancel_definition.call(()); rename.dismiss.call(()); tools.actions.call(()); }, {t!(ws,L,"editor-actions-shortcut")} }
                button { class:"mk-btn mk-native-find-references", title:t!(ws,L,"editor-references-shortcut"), onclick:move |_| { hover_target.set(None); close_completion.call(()); cancel_definition.call(()); rename.dismiss.call(()); tools.references.call(()); }, {t!(ws,L,"editor-references-shortcut")} }
                if comments(model.read().language).is_some() {
                    button { class: "mk-btn mk-native-comment", onclick: move |_| toggle_comment.call(()), {t!(ws, L, "editor-toggle-comment")} }
                }
                if !model.read().structure.folds.is_empty() {
                    button { class: "mk-btn mk-native-fold-all", onclick: move |_| fold.call((None, true)), {t!(ws, L, "editor-fold-all")} }
                    button { class: "mk-btn mk-native-unfold-all", onclick: move |_| fold.call((None, false)), {t!(ws, L, "editor-unfold-all")} }
                }
                if codemirror_on {
                    button {
                        class: "mk-btn mk-editor-switch",
                        title: t!(ws, L, "editor-to-codemirror"),
                        onclick: move |_| {
                            let mut ws = ws;
                            ws.choose_editor(node, "codemirror");
                        },
                        "CodeMirror"
                    }
                }
            }
            crate::native_rename::RenamePrompt { ws, rename }
            crate::native_lsp_tools::ToolsMenu { ws, node, tools }
            if let Some(error) = last_error() {
                div { class: "mk-editor-bar mk-editor-error", "{error}" }
            }
            // Reserve this row even when the caret has no word. Adding it on
            // pointer-down would move the source cells during drag selection.
            div {
                class: "mk-editor-cursor-word",
                aria_hidden: cursor_word.is_none().to_string(),
                if let Some(word) = cursor_word.as_ref() { "‹{word}›" }
            }
            if let Some(replacing) = search_mode() {
                crate::native_search::NativeSearch { ws, model, first_row, highlights, onchange: changed, replacing, onclose: move |_| { search_mode.set(None); focus_request.with_mut(|value| *value += 1); } }
            }
            if !diagnostics.read().is_empty() {
                div { class: "mk-native-diagnostics", role: "region", aria_label: t!(ws, L, "editor-diagnostics"),
                    for diagnostic in diagnostics.read().iter() {
                        button {
                            class: "mk-native-diagnostic-message mk-native-diagnostic-{diagnostic.severity}",
                            onclick: {
                                let line = diagnostic.line;
                                let col = diagnostic.col;
                                move |_| {
                                    let target = doc.peek().node.clone();
                                    focus_request.with_mut(|value| *value += 1);
                                    spawn(async move { let _ = ws.reveal(target, line, col).await; });
                                }
                            },
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
                diagnostics,
                presence: presence(),
                wiki_marks: wiki_marks(),
                onwiki: follow_wiki,
                model,
                highlights,
                first_row,
                first_column,
                scroll_displacement,
                source_anchor,
                run_heights,
                onchange: changed,
                onprepare: prepare_input,
                onhistory: history,
                focus_request,
                onfold: move |(line, collapsed)| fold.call((Some(line), collapsed)),
                fold_label: t!(ws, L, "editor-fold"),
                unfold_label: t!(ws, L, "editor-unfold"),
                label: t!(ws, L, "editor-rust-label"),
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
    let selection = model.with(|state| state.selection(&snapshot.snapshot().text));
    let revision = snapshot.snapshot().revision;
    let (line, column) = model.with(|state| state.cursor_line_col(&snapshot.snapshot().text));
    drop(snapshot);
    if session.with_mut(|state| state.set_selection(revision, selection)) {
        ws.set_editor_selection(node, revision, selection.anchor, selection.head);
        ws.set_cursor(node, line, column);
    }
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
