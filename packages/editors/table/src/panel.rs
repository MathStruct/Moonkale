use crate::grid::{self, Move, HEAD_H, NUM_W, ROW_H};
use crate::L;
use dioxus::prelude::*;
use moonkale_core::{Node, Query, QueryResult, SourceError, Table, TextDialect, Value};
use moonkale_ext_api::{t, Command, GraphRequest, Workspace};
use std::rc::Rc;

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
        // Sophia's own language: every name and what it points at.
        "query" => "select p, d where name(p, d)".to_string(),
        // Milestone 17: key/value tables and Helix labels.
        "kv" => format!("scan {} limit 1000", quote_word(table)),
        "helix" => {
            let (what, label) = match table.strip_prefix("edge:") {
                Some(l) => ("edges", l),
                None => ("nodes", table.strip_prefix("label:").unwrap_or(table)),
            };
            format!("{what} {} limit 1000", quote_word(label))
        }
        _ => format!(
            "SELECT * FROM \"{}\" LIMIT 1000",
            table.replace('"', "\"\"")
        ),
    }
}

/// A name as one word of the `kv`/`helix` dialects: quoted when it has
/// spaces or quotes in it.
fn quote_word(s: &str) -> String {
    if !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || c == '"') {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn dialect_name(d: Option<TextDialect>) -> &'static str {
    d.map(TextDialect::name).unwrap_or("sql")
}

#[component]
pub fn TablePanel(ws: Workspace, node: Node) -> Element {
    let source_id = node.source.clone();
    // Every dialect the source answers, its main one first (a Sophia store
    // offers `sql` and `query`); one button each when there are several.
    let dialects: Vec<&'static str> = ws
        .sources
        .open
        .peek()
        .iter()
        .find(|s| s.descriptor.id == source_id)
        .map(|s| {
            let caps = s.descriptor.capabilities;
            let all: Vec<_> = caps.text_dialects().map(TextDialect::name).collect();
            if all.is_empty() {
                vec![dialect_name(None)]
            } else {
                all
            }
        })
        .unwrap_or_else(|| vec!["sql"]);
    let mut dialect = use_signal(|| dialects[0]);
    let mut text = use_signal(|| default_query(&node, dialects[0]));
    // Shared, not cloned per render: a result can hold 50 000 rows.
    let mut result: Signal<Option<Result<Rc<QueryResult>, SourceError>>> = use_signal(|| None);
    let mut running = use_signal(|| false);
    // Each run's grid starts fresh (widths, sort, filter, scroll).
    let mut runs = use_signal(|| 0u64);

    let run = {
        let source_id = source_id.clone();
        move || {
            let source_id = source_id.clone();
            let text = text.peek().clone();
            let dialect = dialect.peek().to_string();
            spawn(async move {
                running.set(true);
                let out = match ws.source(&source_id) {
                    Some(s) => s.query(Query::Text { dialect, text }).await.and_then(|r| {
                        if r.table.is_none() {
                            Err(SourceError::Invalid("query returned no rows".into()))
                        } else {
                            Ok(r)
                        }
                    }),
                    None => Err(SourceError::NotFound),
                };
                result.set(Some(out.map(Rc::new)));
                runs += 1;
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
            ws.docs.graph_request.set(Some(GraphRequest {
                source: source_id.clone(),
                dialect: dialect.peek().to_string(),
                text: text.peek().clone(),
            }));
            // The Graph tab usually shares this tile; bring it forward.
            ws.dispatch(Command::ShowPanel("graph"));
        }
    };

    let pick = {
        let run = run.clone();
        let node = node.clone();
        move |d: &'static str| {
            let old = *dialect.peek();
            if old == d {
                return;
            }
            // Untouched default text follows the language; edited text stays.
            if *text.peek() == default_query(&node, old) {
                text.set(default_query(&node, d));
            }
            dialect.set(d);
            run();
        }
    };

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div { class: "mk-table",
            div { class: "mk-table-query",
                if dialects.len() > 1 {
                    div { class: "mk-table-dialects", role: "group",
                        for d in dialects.iter().copied() {
                            button {
                                key: "{d}",
                                class: if dialect() == d { "mk-btn mk-btn-on" } else { "mk-btn" },
                                title: t!(ws, L, "table-dialect-title", dialect = d),
                                onclick: {
                                    let mut pick = pick.clone();
                                    move |_| pick(d)
                                },
                                "{d}"
                            }
                        }
                    }
                }
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
                    if running() { {t!(ws, L, "table-running")} } else { {t!(ws, L, "table-run")} }
                }
                if matches!(result(), Some(Ok(ref r)) if !r.nodes.is_empty()) {
                    button { class: "mk-btn mk-table-graph", onclick: show_in_graph, title: t!(ws, L, "table-graph-title"), {t!(ws, L, "table-graph")} }
                }
            }
            div { class: "mk-table-grid-host",
                match result() {
                    None => rsx! { p { class: "mk-table-msg", {t!(ws, L, "table-loading")} } },
                    Some(Err(e)) => rsx! { p { class: "mk-table-msg mk-table-error", "{e}" } },
                    Some(Ok(r)) => rsx! {
                        ResultGrid { key: "{runs}", ws, result: SharedResult(r) }
                    },
                }
            }
        }
    }
}

/// A result handed to the grid by pointer: comparing props must not walk
/// 50 000 rows on every render of the panel.
#[derive(Clone)]
pub struct SharedResult(pub Rc<QueryResult>);

impl PartialEq for SharedResult {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl SharedResult {
    fn table(&self) -> &Table {
        self.0
            .table
            .as_ref()
            .expect("the panel only shows results with rows")
    }
}

/// A cell's class: NULL dimmed, numbers right-aligned, bytes dimmed.
fn cell_class(v: &Value) -> &'static str {
    match v {
        Value::Null => "mk-table-cell mk-cell-null",
        Value::Int(_) | Value::Float(_) => "mk-table-cell mk-cell-num",
        Value::Bytes { .. } => "mk-table-cell mk-cell-bytes",
        _ => "mk-table-cell",
    }
}

/// The result grid: virtualised rows *and* columns — only the cells under
/// the viewport (plus overscan) are in the DOM, so a 50 000-row result
/// scrolls like a 50-row one. Sticky header and row numbers; click a
/// header to sort (ascending, descending, off), drag its right edge to
/// resize, filter with the box above; the keyboard moves a cell cursor
/// (arrows, Page Up/Down, Home/End, Ctrl+Home/End). Sort and filter apply
/// to the rows loaded — the meta line says so when the source truncated.
#[component]
fn ResultGrid(ws: Workspace, result: SharedResult) -> Element {
    let table = result.table();
    let total = table.rows.len();
    let n_cols = table.columns.len();
    let mut widths = use_signal({
        let t = table.clone();
        move || grid::initial_widths(&t.columns, &t.rows)
    });
    let mut sort = use_signal(|| None::<(usize, bool)>);
    let mut filter = use_signal(String::new);
    let mut scroll = use_signal(|| (0.0f64, 0.0f64));
    // Until the first resize report, a generous guess: overscan covers it.
    let mut view = use_signal(|| (1600.0f64, 900.0f64));
    let mut cursor = use_signal(|| None::<(usize, usize)>);
    // A column resize in progress: (column, pointer x at the start, width).
    let mut drag = use_signal(|| None::<(usize, f64, f64)>);
    let mut grid_el = use_signal(|| None::<Rc<MountedData>>);
    let order = use_memo({
        let r = result.clone();
        move || grid::order(&r.table().rows, sort(), &filter.read())
    });

    let shown = order.read().len();
    let lefts = grid::lefts(&widths.read());
    let full_w = lefts[n_cols];
    let (sl, st) = scroll();
    let (vw, vh) = view();
    let rows = grid::row_window(st, vh, shown);
    let cols = grid::col_window(&lefts, sl, vw);
    let cur = cursor();
    let sorted = sort();

    let mut go = move |mv: Move| {
        let n = order.peek().len();
        let to = match *cursor.peek() {
            Some(at) => grid::step(
                at,
                mv,
                n,
                n_cols,
                ((view.peek().1 - HEAD_H) / ROW_H) as usize,
            ),
            None => (0, 0),
        };
        if n == 0 || n_cols == 0 {
            return;
        }
        cursor.set(Some(to));
        let l = grid::lefts(&widths.peek());
        if let Some((x, y)) = grid::reveal(*scroll.peek(), to, &l, *view.peek()) {
            if let Some(el) = grid_el.peek().clone() {
                spawn(async move {
                    let _ = el
                        .scroll(
                            dioxus::html::geometry::PixelsVector2D::new(x, y),
                            ScrollBehavior::Instant,
                        )
                        .await;
                });
            }
        }
    };

    rsx! {
        div { class: "mk-table-meta",
            if shown == total {
                {t!(ws, L, "table-counts", rows = total, columns = n_cols)}
            } else {
                {t!(ws, L, "table-counts-filtered", shown = shown, rows = total, columns = n_cols)}
            }
            if !result.0.nodes.is_empty() { {t!(ws, L, "table-graph-counts", nodes = result.0.nodes.len(), edges = result.0.edges.len())} }
            if table.truncated || result.0.truncated { {t!(ws, L, "table-truncated")} }
            input {
                class: "mk-table-filter",
                r#type: "search",
                placeholder: t!(ws, L, "table-filter"),
                value: "{filter}",
                oninput: move |e| {
                    filter.set(e.value());
                    cursor.set(None);
                },
            }
        }
        div {
            class: "mk-table-grid",
            role: "grid",
            tabindex: "0",
            aria_rowcount: "{shown + 1}",
            aria_colcount: "{n_cols}",
            "data-rows": "{shown}",
            "data-total": "{total}",
            "data-first": "{rows.start}",
            "data-drawn": "{rows.len()}",
            "data-cols-drawn": "{cols.len()}",
            onmounted: move |e| grid_el.set(Some(e.data())),
            onresize: move |e| {
                if let Ok(size) = e.data().get_content_box_size() {
                    view.set((size.width, size.height));
                }
            },
            onscroll: move |e| {
                let d = e.data();
                scroll.set((d.scroll_left(), d.scroll_top()));
                if d.client_width() > 0 {
                    view.set((d.client_width() as f64, d.client_height() as f64));
                }
            },
            onkeydown: move |e| {
                let primary = moonkale_ext_api::keys::primary(&e.modifiers());
                let mv = match e.key() {
                    Key::ArrowUp => Move::Up,
                    Key::ArrowDown => Move::Down,
                    Key::ArrowLeft => Move::Left,
                    Key::ArrowRight => Move::Right,
                    Key::PageUp => Move::PageUp,
                    Key::PageDown => Move::PageDown,
                    Key::Home if primary => Move::First,
                    Key::End if primary => Move::Last,
                    Key::Home => Move::Home,
                    Key::End => Move::End,
                    _ => return,
                };
                e.prevent_default();
                go(mv);
            },
            onmousemove: move |e| {
                if let Some((c, x0, w0)) = drag() {
                    let x = e.client_coordinates().x;
                    if let Some(w) = widths.write().get_mut(c) {
                        *w = (w0 + x - x0).max(grid::MIN_W);
                    }
                }
            },
            onmouseup: move |_| drag.set(None),
            onmouseleave: move |_| drag.set(None),
            div {
                class: "mk-table-sizer",
                style: "height: {HEAD_H + shown as f64 * ROW_H}px; width: {NUM_W + full_w}px;",
                div { class: "mk-table-head", role: "row", style: "width: {NUM_W + full_w}px;",
                    div { class: "mk-table-num mk-table-hnum", "#" }
                    div { class: "mk-table-gap", style: "width: {lefts[cols.start]}px;" }
                    for c in cols.clone() {
                        div {
                            key: "{c}",
                            class: "mk-table-hcell",
                            role: "columnheader",
                            aria_sort: match sorted { Some((s, true)) if s == c => "ascending", Some((s, false)) if s == c => "descending", _ => "none" },
                            style: "width: {widths.read()[c]}px;",
                            title: "{table.columns[c]}",
                            onclick: move |_| { let next = grid::next_sort(*sort.peek(), c); sort.set(next); },
                            span { class: "mk-table-hname", "{table.columns[c]}" }
                            span { class: "mk-table-arrow",
                                match sorted { Some((s, true)) if s == c => "▲", Some((s, false)) if s == c => "▼", _ => "" }
                            }
                            div {
                                class: "mk-table-resize",
                                onclick: move |e| e.stop_propagation(),
                                onmousedown: move |e| {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    let w = widths.peek().get(c).copied().unwrap_or(grid::MIN_W);
                                    drag.set(Some((c, e.client_coordinates().x, w)));
                                },
                            }
                        }
                    }
                }
                div { class: "mk-table-body", style: "top: {HEAD_H + rows.start as f64 * ROW_H}px; width: {NUM_W + full_w}px;",
                    for pos in rows.clone() {
                        {
                            let i = order.read()[pos];
                            let row = &table.rows[i];
                            rsx! {
                                div { key: "{i}", class: "mk-table-row", role: "row", aria_rowindex: "{pos + 2}", "data-row": "{i}",
                                    div { class: "mk-table-num", "{i + 1}" }
                                    div { class: "mk-table-gap", style: "width: {lefts[cols.start]}px;" }
                                    for c in cols.clone() {
                                        {
                                            let v = row.get(c).unwrap_or(&Value::Null);
                                            let text = grid::cell_text(v);
                                            let at = cur == Some((pos, c));
                                            rsx! {
                                                div {
                                                    key: "{c}",
                                                    class: if at { "{cell_class(v)} mk-cell-cursor" } else { "{cell_class(v)}" },
                                                    role: "gridcell",
                                                    style: "width: {widths.read()[c]}px;",
                                                    title: if matches!(v, Value::Text(_)) { "{text}" } else { "" },
                                                    onclick: move |_| cursor.set(Some((pos, c))),
                                                    "{text}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
