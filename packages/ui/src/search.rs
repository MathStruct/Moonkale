//! Search panel (Milestone 4): hybrid search over the index — a query box,
//! hits with path · line · snippet, click opens the file at the line.

use dioxus::prelude::*;
use moonkale_core::{Node, Query, Value};
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::Command;

pub const PANEL_ID: &str = "search";
const INPUT_ID: &str = "mk-search-input";

pub struct SearchExtension;

impl Extension for SearchExtension {
    fn manifest(&self) -> Manifest {
        Manifest::core(
            "dev.moonkale.search",
            "Search",
            "Keyword + semantic search over the open folder (Ctrl+Shift+F).",
        )
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Search".into(),
            home: PanelHome::Side,
            closable: false,
            dirty: false,
            node: None,
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { SearchPanel { ws } }
    }
}

/// Bring the panel forward and focus its box (Ctrl+Shift+F).
pub fn focus_search(mut ws: Workspace) {
    ws.dispatch(Command::ShowPanel(PANEL_ID));
    dioxus_workbench::focus_after_render(INPUT_ID);
}

#[derive(Clone, PartialEq)]
struct Hit {
    node: Option<Node>,
    path: String,
    line: u32,
    snippet: String,
}

#[component]
fn SearchPanel(ws: Workspace) -> Element {
    let mut query = use_signal(String::new);
    let mut hits: Signal<Option<Result<Vec<Hit>, String>>> = use_signal(|| None);
    let mut running = use_signal(|| false);

    let mut run = move || {
        let q = query.peek().trim().to_string();
        if q.is_empty() {
            hits.set(None);
            return;
        }
        let Some(index) = ws.index() else {
            hits.set(Some(Err("Open a folder first.".into())));
            return;
        };
        spawn(async move {
            running.set(true);
            let out = index
                .source
                .query(Query::Text {
                    dialect: "search".into(),
                    text: q,
                })
                .await
                .map(|res| {
                    let table = res.table.unwrap_or_default();
                    table
                        .rows
                        .iter()
                        .map(|row| {
                            let path = row.first().map(|v| v.to_string()).unwrap_or_default();
                            let line = match row.get(1) {
                                Some(Value::Int(i)) => *i as u32,
                                _ => 1,
                            };
                            Hit {
                                node: res.nodes.iter().find(|n| n.native_key == path).cloned(),
                                path,
                                line,
                                snippet: row.get(3).map(|v| v.to_string()).unwrap_or_default(),
                            }
                        })
                        .collect()
                })
                .map_err(|e| e.to_string());
            hits.set(Some(out));
            running.set(false);
        });
    };

    // Replace (Milestone 7): literal occurrences of the query in the files
    // the search found; open documents take the edit unsaved, closed files
    // are written through the source.
    let mut replacement = use_signal(String::new);
    let mut preview: Signal<Option<Vec<(Node, usize)>>> = use_signal(|| None);
    let mut replacing = use_signal(|| false);
    let hit_nodes = move || -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        if let Some(Ok(list)) = hits.peek().as_ref() {
            for h in list {
                if let Some(n) = &h.node {
                    if !out.iter().any(|o| o.id == n.id) {
                        out.push(n.clone());
                    }
                }
            }
        }
        out
    };
    let do_preview = move || {
        let needle = query.peek().clone();
        let nodes = hit_nodes();
        spawn(async move {
            let mut out = Vec::new();
            for n in nodes {
                if let Ok(c) = ws.count_occurrences(&n, &needle).await {
                    if c > 0 {
                        out.push((n, c));
                    }
                }
            }
            preview.set(Some(out));
        });
    };
    let do_replace = move |_| {
        let needle = query.peek().clone();
        let with = replacement.peek().clone();
        let Some(files) = preview.peek().clone() else {
            return;
        };
        spawn(async move {
            replacing.set(true);
            let mut total = 0;
            let mut failed = 0;
            for (n, _) in files {
                match ws.replace_in_file(&n, &needle, &with).await {
                    Ok(c) => total += c,
                    Err(e) => {
                        failed += 1;
                        ws.set_status(format!("{}: {e}", n.native_key));
                    }
                }
            }
            if failed == 0 {
                ws.set_status(format!("Replaced {total} occurrence(s) of {needle:?}"));
            }
            preview.set(None);
            replacing.set(false);
            run();
        });
    };
    let preview_now = preview.read().clone();
    let preview_total: usize = preview_now
        .as_ref()
        .map(|p| p.iter().map(|(_, c)| c).sum())
        .unwrap_or(0);

    rsx! {
        document::Stylesheet { href: crate::explorer::EXPLORER_CSS }
        div { class: "mk-search",
            div { class: "mk-search-box",
                input {
                    id: INPUT_ID,
                    class: "mk-input",
                    placeholder: "Search files… (Enter)",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            e.prevent_default();
                            run();
                        }
                    },
                }
                button { class: "mk-btn", disabled: running(), onclick: move |_| run(), "Go" }
            }
            if matches!(hits(), Some(Ok(ref l)) if !l.is_empty()) {
                div { class: "mk-search-box mk-search-replace",
                    input {
                        class: "mk-input",
                        placeholder: "Replace with…",
                        value: "{replacement}",
                        oninput: move |e| { replacement.set(e.value()); preview.set(None); },
                        onkeydown: move |e: KeyboardEvent| { if e.key() == Key::Enter { e.prevent_default(); do_preview(); } },
                    }
                    button { class: "mk-btn", disabled: replacing(), onclick: move |_| do_preview(), "Preview" }
                }
                if let Some(files) = preview_now {
                    div { class: "mk-search-preview", "data-files": "{files.len()}", "data-total": "{preview_total}",
                        if files.is_empty() {
                            p { class: "mk-muted", "No literal occurrences of the query in the found files." }
                        } else {
                            p { "Replace " b { "{preview_total}" } " occurrence(s) of " code { "{query}" } " with " code { "{replacement}" } " in:" }
                            ul {
                                for (n, c) in files.iter() {
                                    li { key: "{n.id}", "{n.native_key} " span { class: "mk-muted", "({c})" } if ws.document(n.id).is_some() { span { class: "mk-muted", " — open, stays unsaved" } } }
                                }
                            }
                            button { class: "mk-btn mk-btn-on", disabled: replacing(), onclick: do_replace, "Replace all" }
                        }
                    }
                }
            }
            div { class: "mk-search-results",
                match hits() {
                    None => rsx! { p { class: "mk-muted", "Keyword + semantic search over the open folder." } },
                    Some(Err(e)) => rsx! { p { class: "mk-explorer-error", "{e}" } },
                    Some(Ok(list)) if list.is_empty() => rsx! { p { class: "mk-muted", "No matches." } },
                    Some(Ok(list)) => rsx! {
                        for (i, h) in list.into_iter().enumerate() {
                            div { key: "{i}", class: "mk-search-hit", title: "{h.path}:{h.line}",
                                onclick: move |_| {
                                    let h = h.clone();
                                    spawn(async move {
                                        let node = match h.node.clone() {
                                            Some(n) => Some(n),
                                            None => ws.open_relative_path(&h.path).await.ok(),
                                        };
                                        if let Some(n) = node {
                                            let _ = ws.reveal(n, h.line.saturating_sub(1), 0).await;
                                        }
                                    });
                                },
                                div { class: "mk-search-hit-path", "{h.path}" span { class: "mk-search-hit-line", ":{h.line}" } }
                                div { class: "mk-search-hit-snippet", "{h.snippet}" }
                            }
                        }
                    },
                }
            }
        }
    }
}
