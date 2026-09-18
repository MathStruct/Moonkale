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
        Manifest::core("dev.moonkale.search", "Search", "Keyword + semantic search over the open folder (Ctrl+Shift+F).")
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
