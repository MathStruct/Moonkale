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
}

#[component]
pub fn Palette() -> Element {
    let mut ws = use_context::<Workspace>();
    let mut state = use_context::<PaletteState>();
    let reg = use_context::<CommandRegistry>();
    let seed = use_context::<SearchSeed>().0;
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
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
        if mode.is_some() {
            dioxus_workbench::focus_after_render(INPUT_ID);
        }
        if matches!(mode, Some(PaletteMode::Files | PaletteMode::Search)) {
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
        PaletteMode::Commands | PaletteMode::Search => (q.clone(), None),
    };
    let mut ranked: Vec<(i32, Item)> = match mode {
        PaletteMode::Commands | PaletteMode::Search => reg
            .read()
            .entries
            .iter()
            .filter_map(|e| {
                fuzzy_score(&needle, &e.title).map(|s| {
                    (
                        s,
                        Item {
                            command: true,
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
                            key: p.clone(),
                            label: name,
                            detail: dir,
                        },
                    )
                })
            })
            .collect(),
    };
    if mode == PaletteMode::Search {
        ranked.extend(files.read().iter().filter_map(|path| {
            fuzzy_score(&needle, path).map(|score| {
                (
                    score,
                    Item {
                        command: false,
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
    ranked.truncate(60);
    let items: Vec<Item> = ranked.into_iter().map(|(_, i)| i).collect();
    let sel = (*selected.read()).min(items.len().saturating_sub(1));

    let choose = Callback::new({
        let items = items.clone();
        move |(i, background): (usize, bool)| {
            let Some(item) = items.get(i).cloned() else {
                return;
            };
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
    let count = items.len();
    let placeholder = match mode {
        PaletteMode::Commands => t!(ws, L, "palette-commands"),
        PaletteMode::Files => t!(ws, L, "palette-files"),
        PaletteMode::Search => t!(ws, L, "discovery-search"),
    };

    rsx! {
        div { class: "mk-palette-backdrop", onclick: move |_| state.set(None),
            div { class: "mk-palette", role: "dialog", "aria-modal": "true", "aria-label": "Command palette", onclick: |e| e.stop_propagation(),
                input {
                    id: INPUT_ID,
                    class: "mk-palette-input",
                    placeholder,
                    value: "{q}",
                    autocomplete: "off",
                    oninput: move |e| { query.set(e.value()); selected.set(0); },
                    onkeydown: move |e| {
                        if e.is_composing() { return; } // the IME's keys (#19)
                        match e.key() {
                            Key::ArrowDown => { e.prevent_default(); selected.set((sel + 1).min(count.saturating_sub(1))); }
                            Key::ArrowUp => { e.prevent_default(); selected.set(sel.saturating_sub(1)); }
                            Key::Enter => { e.prevent_default(); choose.call((sel, e.modifiers().alt())); }
                            Key::Escape => { e.prevent_default(); state.set(None); }
                            _ => {}
                        }
                        // Keep Ctrl+P / Ctrl+Shift+P from re-opening underneath.
                        e.stop_propagation();
                    },
                }
                ul { class: "mk-palette-list", role: "listbox",
                    for (i, item) in items.iter().enumerate() {
                        li {
                            key: "{item.key}",
                            class: if i == sel { "mk-palette-item mk-selected" } else { "mk-palette-item" },
                            role: "option",
                            "aria-selected": i == sel,
                            "data-key": "{item.key}",
                            "data-command-id": if item.command { item.key.as_str() } else { "" },
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| choose.call((i, false)),
                            onauxclick: move |e| {
                                if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Auxiliary) { e.prevent_default(); choose.call((i, true)); }
                            },
                            span { class: "mk-palette-label", "{item.label}" }
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
