//! Command palette (`Ctrl+Shift+P`) and quick open (`Ctrl+P`), Milestone 7:
//! one overlay, two item sources, fuzzy-ranked, keyboard-driven.

use crate::commands::{self, CommandRegistry};
use crate::L;
use dioxus::prelude::*;
use moonkale_core::{NodeKind, Query};
use moonkale_ext_api::{fuzzy_score, t, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    Commands,
    Files,
    Search,
    Overview,
}

/// `Some(mode)` while open. Provided by the frame (`use_context`).
pub type PaletteState = Signal<Option<PaletteMode>>;

#[derive(Clone, Copy)]
pub struct SearchSeed(pub Signal<String>);

pub const INPUT_ID: &str = "mk-palette-input";

#[derive(Clone, PartialEq)]
struct Item {
    /// Command id or file path.
    key: String,
    label: String,
    detail: String,
    command: bool,
    tab: Option<crate::overview::Tab>,
}

#[component]
pub fn Palette(
    tabs: Vec<crate::overview::Tab>,
    on_switch: Callback<(String, bool)>,
    on_close: Callback<String>,
) -> Element {
    let mut ws = use_context::<Workspace>();
    let mut state = use_context::<PaletteState>();
    let reg = use_context::<CommandRegistry>();
    let seed = use_context::<SearchSeed>().0;
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut confirming = use_signal(|| None::<String>);
    // Quick open: the folder's files from the index, loaded when opening.
    let mut files: Signal<Vec<String>> = use_signal(Vec::new);

    let mode = *state.read();
    use_effect(move || {
        let mode = *state.read();
        query.set(if mode == Some(PaletteMode::Search) {
            seed.peek().clone()
        } else {
            String::new()
        });
        selected.set(0);
        confirming.set(None);
        if mode.is_some() {
            dioxus_workbench::focus_after_render(INPUT_ID);
        }
        if matches!(
            mode,
            Some(PaletteMode::Files | PaletteMode::Search | PaletteMode::Overview)
        ) {
            let Some(index) = ws.index() else {
                files.set(Vec::new());
                return;
            };
            spawn(async move {
                let res = index
                    .source
                    .query(Query::All {
                        limit: 50_000,
                        kinds: Some(vec![NodeKind::File]),
                    })
                    .await;
                let mut list: Vec<String> = res
                    .map(|r| r.nodes.into_iter().map(|n| n.native_key).collect())
                    .unwrap_or_default();
                list.sort();
                files.set(list);
            });
        }
    });

    use_effect(move || {
        let _ = selected();
        if state.read().is_some() {
            let _ = document::eval("document.querySelector('#mk-palette-dialog [aria-selected=\"true\"]')?.scrollIntoView({block:'nearest'});");
        }
    });

    let Some(mode) = mode else {
        return rsx! {};
    };

    // Rank. A `:line` suffix in quick open reveals that line.
    let q = query.read().clone();
    let (needle, goto_line) = match mode {
        PaletteMode::Files => match q.rsplit_once(':') {
            Some((p, l)) if l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty() => {
                (p.to_string(), l.parse::<u32>().ok())
            }
            _ => (q.clone(), None),
        },
        PaletteMode::Commands | PaletteMode::Search | PaletteMode::Overview => (q.clone(), None),
    };
    let mut ranked: Vec<(i32, Item)> = match mode {
        PaletteMode::Commands | PaletteMode::Search | PaletteMode::Overview => reg
            .read()
            .entries
            .iter()
            .filter_map(|e| {
                fuzzy_score(&needle, &e.title).map(|s| {
                    (
                        s,
                        Item {
                            command: true,
                            tab: None,
                            key: e.id.clone(),
                            label: e.title.clone(),
                            detail: moonkale_ext_api::command_hint(&e.id, "").shortcut,
                        },
                    )
                })
            })
            .collect(),
        PaletteMode::Files => files
            .read()
            .iter()
            .filter_map(|p| {
                fuzzy_score(&needle, p).map(|s| {
                    let (dir, name) = p
                        .rsplit_once('/')
                        .map(|(d, n)| (d.to_string(), n.to_string()))
                        .unwrap_or((String::new(), p.clone()));
                    (
                        s,
                        Item {
                            command: false,
                            tab: None,
                            key: p.clone(),
                            label: name,
                            detail: dir,
                        },
                    )
                })
            })
            .collect(),
    };
    if matches!(mode, PaletteMode::Search | PaletteMode::Overview) {
        ranked.extend(files.read().iter().filter_map(|path| {
            fuzzy_score(&needle, path).map(|score| {
                (
                    score,
                    Item {
                        command: false,
                        tab: None,
                        key: path.clone(),
                        label: path.rsplit('/').next().unwrap_or(path).to_string(),
                        detail: path.clone(),
                    },
                )
            })
        }));
    }
    if needle.is_empty() {
        // Keep the natural order (registry order, alphabetical paths).
    } else {
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
    }
    // The unfiltered command palette is the complete registry. Search/file
    // results remain capped; adding a shell command must not hide extensions.
    if mode != PaletteMode::Commands || !needle.is_empty() {
        ranked.truncate(60);
    }
    let mut items: Vec<Item> = Vec::new();
    if matches!(mode, PaletteMode::Overview | PaletteMode::Search) {
        items.extend(
            crate::overview::grouped(&tabs, &q)
                .into_iter()
                .map(|tab| Item {
                    key: tab.id.clone(),
                    label: tab.title.clone(),
                    detail: tab.path.clone(),
                    command: false,
                    tab: Some(tab),
                }),
        );
    }
    let tab_count = items.len();
    items.extend(ranked.into_iter().map(|(_, i)| i));
    let sel = (*selected.read()).min(items.len().saturating_sub(1));

    let choose = Callback::new({
        let items = items.clone();
        move |(i, background): (usize, bool)| {
            let Some(item) = items.get(i).cloned() else {
                return;
            };
            if let Some(tab) = item.tab {
                state.set(None);
                on_switch.call((tab.id, background));
                return;
            }
            if background && item.command {
                return;
            }
            state.set(None);
            if item.command {
                commands::run(&item.key, ws);
            } else {
                let path = item.key.clone();
                spawn(async move {
                    if background {
                        let _ = ws.load_relative_path(&path).await;
                        return;
                    }
                    match ws.open_relative_path(&path).await {
                        Ok(node) => {
                            if let Some(line) = goto_line {
                                let _ = ws.reveal(node, line.saturating_sub(1), 0).await;
                            }
                        }
                        Err(e) => {
                            ws.set_status(t!(ws, L, "palette-open-failed", error = e.to_string()))
                        }
                    }
                });
            }
        }
    });
    let close = Callback::new({
        let items = items.clone();
        move |i: usize| {
            if let Some(tab) = items.get(i).and_then(|i| i.tab.as_ref()) {
                if tab.dirty {
                    confirming.set(Some(tab.id.clone()));
                } else {
                    on_close.call(tab.id.clone());
                }
            }
        }
    });
    let count = items.len();
    let groups: Vec<_> = items
        .iter()
        .map(|i| i.tab.as_ref().map(|t| t.group.clone()))
        .collect();
    let placeholder = match mode {
        PaletteMode::Commands => t!(ws, L, "palette-commands"),
        PaletteMode::Files => t!(ws, L, "palette-files"),
        PaletteMode::Search | PaletteMode::Overview => t!(ws, L, "discovery-search"),
    };

    rsx! {
        div { class: "mk-palette-backdrop", onclick: move |_| state.set(None),
            div { id: "mk-palette-dialog", class: if matches!(mode, PaletteMode::Overview | PaletteMode::Search) { "mk-palette mk-overview" } else { "mk-palette" }, role: "dialog", "aria-modal": "true", "aria-label": t!(ws, L, "overview-title"), onclick: |e| e.stop_propagation(),
                input {
                    id: INPUT_ID,
                    role: "combobox",
                    "aria-expanded": "true",
                    "aria-controls": "mk-palette-results",
                    "aria-activedescendant": "mk-palette-option-{sel}",
                    class: "mk-palette-input",
                    placeholder,
                    value: "{q}",
                    autocomplete: "off",
                    oninput: move |e| { query.set(e.value().chars().take(256).collect()); selected.set(0); },
                    onkeydown: move |e| {
                        if e.is_composing() { return; } // the IME's keys (#19)
                        match e.key() {
                            Key::ArrowRight if tab_count > 0 => { e.prevent_default(); selected.set((sel + 1).min(count.saturating_sub(1))); }
                            Key::ArrowDown => { e.prevent_default(); selected.set((sel + 1).min(count.saturating_sub(1))); }
                            Key::ArrowLeft if tab_count > 0 => { e.prevent_default(); selected.set(sel.saturating_sub(1)); }
                            Key::ArrowUp => { e.prevent_default(); selected.set(sel.saturating_sub(1)); }
                            Key::Home => { e.prevent_default(); selected.set(0); }
                            Key::End => { e.prevent_default(); selected.set(count.saturating_sub(1)); }
                            Key::Delete if sel < tab_count => { e.prevent_default(); close.call(sel); }
                            Key::Tab if sel < tab_count && !e.modifiers().shift() => {
                                if let Some(next) = (sel+1..count).find(|&i| groups[i] != groups[sel]) { e.prevent_default(); selected.set(next); }
                            }
                            Key::Enter => { e.prevent_default(); choose.call((sel, if sel < tab_count { e.modifiers().shift() } else { e.modifiers().alt() })); }
                            Key::Escape => { e.prevent_default(); state.set(None); }
                            _ => {}
                        }
                        // Keep Ctrl+P / Ctrl+Shift+P from re-opening underneath.
                        e.stop_propagation();
                    },
                }
                if let Some(id) = confirming() {
                    div { class: "mk-overview-confirm", role: "alertdialog", "aria-label": t!(ws, L, "overview-discard"),
                        onkeydown: move |e| { if e.key() == Key::Escape { e.prevent_default(); e.stop_propagation(); confirming.set(None); dioxus_workbench::focus_after_render(INPUT_ID); } },
                        p { {t!(ws, L, "overview-discard")} }
                        button { class: "mk-btn", autofocus: true, onclick: move |_| { on_close.call(id.clone()); confirming.set(None); }, {t!(ws, L, "overview-close")} }
                        button { class: "mk-btn", onclick: move |_| confirming.set(None), {t!(ws, L, "link-cancel")} }
                    }
                }
                if matches!(mode, PaletteMode::Overview | PaletteMode::Search) { h2 { {t!(ws, L, "overview-window")} } }
                ul { id: "mk-palette-results", class: "mk-palette-list", role: "listbox",
                    for (i, item) in items.iter().enumerate() {
                        li {
                            key: "{item.key}",
                            id: "mk-palette-option-{i}",
                            class: if item.tab.is_some() { if i == sel { "mk-palette-item mk-overview-card mk-selected" } else { "mk-palette-item mk-overview-card" } } else if i == sel { "mk-palette-item mk-selected" } else { "mk-palette-item" },
                            "data-overview-card": item.tab.is_some(),
                            "data-kind": item.tab.as_ref().map(|t| t.kind.as_str()).unwrap_or(""),
                            "data-link-group": item.tab.as_ref().map(|t| t.group.as_str()).unwrap_or(""),
                            "aria-label": item.tab.as_ref().map(|t| format!("{} {} {} {} {}", t.title, t.kind, t.marker, t.state, t.mark)).unwrap_or_else(|| item.label.clone()),
                            role: "option",
                            "aria-selected": i == sel,
                            "data-key": "{item.key}",
                            "data-command-id": if item.command { item.key.as_str() } else { "" },
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| choose.call((i, false)),
                            onauxclick: move |e| {
                                if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Auxiliary) { e.prevent_default(); if i < tab_count { close.call(i); } else { choose.call((i, true)); } }
                            },
                            if let Some(tab) = &item.tab {
                                crate::overview::Preview { key: "{tab.id}", tab: tab.clone() }
                                div { class: "mk-overview-card-title",
                                    span { class: "mk-link-marker mk-link-{tab.colour}", title: "{tab.marker}", "{tab.marker}" }
                                    span { class: "mk-palette-label", "◇ {item.label}" }
                                    if tab.dirty { span { title: "{tab.state}", "●" } }
                                    span { "{tab.mark}" }
                                    span { class: "mk-overview-source", style: "background: {tab.source_colour}" }
                                }
                                if !tab.presence.is_empty() { span { "{tab.presence}" } }
                                button { class: "mk-btn mk-overview-close", aria_label: t!(ws, L, "overview-close"), onclick: move |e| { e.stop_propagation(); close.call(i); }, "×" }
                            } else { span { class: "mk-palette-label", "{item.label}" } }
                            span { class: "mk-palette-detail", "{item.detail}" }
                        }
                    }
                    if items.is_empty() {
                        li { class: "mk-palette-empty", if mode == PaletteMode::Files && files.read().is_empty() { "No folder open" } else { "No matches" } }
                    }
                }
            }
        }
    }
}
