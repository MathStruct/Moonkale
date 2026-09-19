use dioxus::prelude::*;
use moonkale_core::{Node, Query, QueryResult, SourceError, TextDialect};
use moonkale_ext_api::{Command, GraphRequest, Workspace};

const CSS: Asset = asset!("/assets/table.css");

/// The query box starts with "everything in this table" in the source's
/// language.
fn default_query(node: &Node, dialect: &str) -> String {
    let table = node
        .native_key
        .strip_prefix("table:")
        .unwrap_or(&node.native_key);
    match dialect {
        "cypher" => {
            // Rel tables are listed as "Name (rel)" in the explorer; the
            // native key is the bare name either way.
            format!("MATCH (n:{table}) RETURN n LIMIT 200")
        }
        _ => format!("SELECT * FROM \"{}\" LIMIT 200", table.replace('"', "\"\"")),
    }
}

fn dialect_name(d: Option<TextDialect>) -> &'static str {
    match d {
        Some(TextDialect::Cypher) => "cypher",
        Some(TextDialect::TypeQl) => "typeql",
        _ => "sql",
    }
}

#[component]
pub fn TablePanel(ws: Workspace, node: Node) -> Element {
    let source_id = node.source.clone();
    let dialect = ws
        .sources
        .peek()
        .iter()
        .find(|s| s.descriptor.id == source_id)
        .map(|s| dialect_name(s.descriptor.capabilities.text_query))
        .unwrap_or("sql");
    let mut text = use_signal(|| default_query(&node, dialect));
    let mut result: Signal<Option<Result<QueryResult, SourceError>>> = use_signal(|| None);
    let mut running = use_signal(|| false);

    let run = {
        let source_id = source_id.clone();
        move || {
            let source_id = source_id.clone();
            let text = text.peek().clone();
            spawn(async move {
                running.set(true);
                let out = match ws.source(&source_id) {
                    Some(s) => s
                        .query(Query::Text {
                            dialect: dialect.into(),
                            text,
                        })
                        .await
                        .and_then(|r| {
                            if r.table.is_none() {
                                Err(SourceError::Invalid("query returned no rows".into()))
                            } else {
                                Ok(r)
                            }
                        }),
                    None => Err(SourceError::NotFound),
                };
                result.set(Some(out));
                running.set(false);
            });
        }
    };

    // First load.
    use_hook({
        let run = run.clone();
        move || run()
    });

    let show_in_graph = {
        let source_id = source_id.clone();
        move |_| {
            let mut ws = ws;
            ws.graph_request.set(Some(GraphRequest {
                source: source_id.clone(),
                dialect: dialect.into(),
                text: text.peek().clone(),
            }));
            // The Graph tab usually shares this tile; bring it forward.
            ws.dispatch(Command::ShowPanel("graph"));
        }
    };

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-table",
            div { class: "mk-table-query",
                textarea {
                    class: "mk-table-sql",
                    rows: 2,
                    spellcheck: false,
                    placeholder: "{dialect}",
                    value: "{text}",
                    oninput: move |e| text.set(e.value()),
                    onkeydown: {
                        let run = run.clone();
                        move |e: KeyboardEvent| {
                            if e.modifiers().ctrl() && e.key() == Key::Enter {
                                e.prevent_default();
                                run();
                            }
                        }
                    },
                }
                button { class: "mk-btn", disabled: running(), onclick: { let run = run.clone(); move |_| run() },
                    if running() { "Running…" } else { "Run (Ctrl+Enter)" }
                }
                if matches!(result(), Some(Ok(ref r)) if !r.nodes.is_empty()) {
                    button { class: "mk-btn mk-table-graph", onclick: show_in_graph, title: "Draw the nodes and edges this query returns in the Graph panel", "Show in Graph" }
                }
            }
            div { class: "mk-table-grid-host",
                match result() {
                    None => rsx! { p { class: "mk-table-msg", "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "mk-table-msg mk-table-error", "{e}" } },
                    Some(Ok(r)) => {
                        let t = r.table.as_ref().unwrap();
                        rsx! {
                        div { class: "mk-table-meta",
                            "{t.rows.len()} rows · {t.columns.len()} columns"
                            if !r.nodes.is_empty() { " · {r.nodes.len()} nodes · {r.edges.len()} edges" }
                            if t.truncated { " · truncated at the source's row cap" }
                        }
                        table { class: "mk-table-grid",
                            thead { tr { for c in &t.columns { th { "{c}" } } } }
                            tbody {
                                for (ri, row) in t.rows.iter().enumerate() {
                                    tr { key: "{ri}",
                                        for (ci, v) in row.iter().enumerate() {
                                            td { key: "{ci}", class: match v { moonkale_core::Value::Null => "mk-cell-null", moonkale_core::Value::Int(_) | moonkale_core::Value::Float(_) => "mk-cell-num", _ => "" }, "{v}" }
                                        }
                                    }
                                }
                            }
                        }
                        }
                    },
                }
            }
        }
    }
}
