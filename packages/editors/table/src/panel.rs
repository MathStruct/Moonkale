use dioxus::prelude::*;
use moonkale_core::{Node, Query, SourceError, Table};
use moonkale_ext_api::Workspace;

const CSS: Asset = asset!("/assets/table.css");

fn default_sql(node: &Node) -> String {
    let table = node
        .native_key
        .strip_prefix("table:")
        .unwrap_or(&node.native_key)
        .replace('"', "\"\"");
    format!("SELECT * FROM \"{table}\" LIMIT 200")
}

#[component]
pub fn TablePanel(ws: Workspace, node: Node) -> Element {
    let mut sql = use_signal(|| default_sql(&node));
    let mut result: Signal<Option<Result<Table, SourceError>>> = use_signal(|| None);
    let mut running = use_signal(|| false);
    let source_id = node.source.clone();

    let run = {
        let source_id = source_id.clone();
        move || {
            let source_id = source_id.clone();
            let text = sql.peek().clone();
            spawn(async move {
                running.set(true);
                let out = match ws.source(&source_id) {
                    Some(s) => s
                        .query(Query::Text {
                            dialect: "sql".into(),
                            text,
                        })
                        .await
                        .and_then(|r| {
                            r.table
                                .ok_or(SourceError::Invalid("query returned no rows".into()))
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

    rsx! {
        document::Stylesheet { href: CSS }
        div { class: "mk-table",
            div { class: "mk-table-query",
                textarea {
                    class: "mk-table-sql",
                    rows: 2,
                    spellcheck: false,
                    value: "{sql}",
                    oninput: move |e| sql.set(e.value()),
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
            }
            div { class: "mk-table-grid-host",
                match result() {
                    None => rsx! { p { class: "mk-table-msg", "Loading…" } },
                    Some(Err(e)) => rsx! { p { class: "mk-table-msg mk-table-error", "{e}" } },
                    Some(Ok(t)) => rsx! {
                        div { class: "mk-table-meta",
                            "{t.rows.len()} rows · {t.columns.len()} columns"
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
                    },
                }
            }
        }
    }
}
