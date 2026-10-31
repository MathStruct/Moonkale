use crate::edit::{Editing, RowsContext, SharedRow};
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
    let instance = use_hook(|| moonkale_ext_api::links::panel_instance(ws.content_panel(node.id)));
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
    let mut result: Signal<Option<Result<Rc<QueryResult>, SourceError>>> =
        use_signal(|| ws.links.results.peek().get(&node.id).cloned().map(Ok));
    // Each new data set receives a fresh keyed grid.
    let mut runs = use_signal(|| 0u64);
    let agent_result = node.source.as_str().starts_with("agent:");
    let owner = node.id;
    moonkale_ext_api::crash::use_effect(move || {
        if agent_result {
            if let Some(rows) = ws.links.results.read().get(&owner).cloned() {
                let same =
                    matches!(&*result.peek(), Some(Ok(current)) if Rc::ptr_eq(current, &rows));
                if !same {
                    result.set(Some(Ok(rows)));
                    runs += 1;
                }
            }
        }
    });
    let mut running = use_signal(|| false);
    let mut row_context = use_signal(|| None::<RowsContext>);
    let mut row_refusal = use_signal(String::new);

    let run = {
        let source_id = source_id.clone();
        let owner = node.id;
        let node = node.clone();
        move || {
            let source_id = source_id.clone();
            let text = text.peek().clone();
            let dialect = dialect.peek().to_string();
            let node = node.clone();
            moonkale_ext_api::crash::spawn(async move {
                running.set(true);
                row_context.set(None);
                row_refusal.set(String::new());
                if let Some(source) = ws.source(&source_id) {
                    if source.descriptor().family == moonkale_core::SourceFamily::Sql
                        && dialect == "sql"
                        && text == default_query(&node, "sql")
                    {
                        let table = node
                            .native_key
                            .strip_prefix("table:")
                            .unwrap_or(&node.native_key)
                            .to_string();
                        match source.read_rows(table.clone(), 1000).await {
                            Ok(page) => {
                                if page.refusal.is_none()
                                    && (source.descriptor().capabilities.atomic_rows
                                        || source.descriptor().capabilities.table_query.keyset)
                                {
                                    let data = Table {
                                        columns: page.columns.clone(),
                                        rows: page
                                            .rows
                                            .iter()
                                            .map(|r| {
                                                page.columns
                                                    .iter()
                                                    .map(|c| r.values[c].clone())
                                                    .collect()
                                            })
                                            .collect(),
                                        truncated: page.truncated,
                                    };
                                    row_context.set(Some(RowsContext {
                                        source: source_id,
                                        handle: source.clone(),
                                        table,
                                        page: Rc::new(page),
                                    }));
                                    result.set(Some(Ok(Rc::new(QueryResult {
                                        table: Some(data),
                                        ..Default::default()
                                    }))));
                                    runs += 1;
                                    running.set(false);
                                    return;
                                }
                                row_refusal.set(
                                    page.refusal.unwrap_or_else(|| t!(ws, L, "table-readonly")),
                                );
                            }
                            Err(error) => row_refusal.set(error.to_string()),
                        }
                    }
                }
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
                let out = out.map(Rc::new);
                if let Ok(rows) = &out {
                    let mut ws = ws;
                    ws.links.results.with_mut(|all| {
                        all.insert(owner, rows.clone());
                    });
                }
                result.set(Some(out));
                runs += 1;
                running.set(false);
            });
        }
    };

    moonkale_ext_api::crash::use_effect({
        let run = run.clone();
        let node_id = node.id;
        move || {
            if matches!(ws.shell.commands.read().1, Some(Command::RunDocument))
                && *ws.docs.active.peek() == Some(node_id)
                && !*running.peek()
            {
                run();
            }
        }
    });
    // First load.
    use_hook({
        let run = run.clone();
        move || {
            if !ws.links.results.peek().contains_key(&node.id) {
                run();
            }
        }
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
            if !agent_result { div { class: "mk-table-query",

                textarea {
                    class: "mk-table-sql",
                    rows: 2,
                    spellcheck: false,
                    placeholder: "{dialect}",
                    value: "{text}",
                    oninput: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| text.set(e.value())),
                    onkeydown: moonkale_ext_api::crash::event({
                        let run = run.clone();
                        move |e: KeyboardEvent| {
                            if e.modifiers().ctrl() && e.key() == Key::Enter {
                                e.prevent_default();
                                run();
                            }
                        }
                    }),
                }
                moonkale_ext_api::icons::ToolbarActions {
                    primary: rsx! {                 button { class: "mk-btn", disabled: running(), onclick: moonkale_ext_api::crash::event({ let run = run.clone(); move |_| run() }),
                    if running() { {t!(ws, L, "table-running")} } else { {t!(ws, L, "table-run")} }
                } },
                moonkale_ext_api::result::ResultMenu {ws,panel:instance.clone()}
                if dialects.len() > 1 {
                    div { class: "mk-table-dialects", role: "group",
                        for d in dialects.iter().copied() {
                            button {
                                key: "{d}",
                                class: if dialect() == d { "mk-btn mk-btn-on" } else { "mk-btn" },
                                title: t!(ws, L, "table-dialect-title", dialect = d),
                                onclick: moonkale_ext_api::crash::event({
                                    let mut pick = pick.clone();
                                    move |_| pick(d)
                                }),
                                "{d}"
                            }
                        }
                    }
                }
                if matches!(result(), Some(Ok(ref r)) if !r.nodes.is_empty()) {
                    button { class: "mk-btn mk-table-graph", onclick: moonkale_ext_api::crash::event(show_in_graph), title: t!(ws, L, "table-graph-title"), {t!(ws, L, "table-graph")} }
                }
                }
            }
            }
            if agent_result {
                details {class:"mk-table-result-menu",summary {"aria-label":t!(ws,L,"table-result-actions"),"⋯"}
                    moonkale_ext_api::result::ResultMenu {ws,panel:instance.clone()}
                }
            }
            if !row_refusal.read().is_empty() { p { class: "mk-table-readonly", role: "status", "{row_refusal}" } }
            div { class: "mk-table-grid-host",
                match result() {
                    None => rsx! { p { class: "mk-table-msg", {t!(ws, L, "table-loading")} } },
                    Some(Err(e)) => rsx! { p { class: "mk-table-msg mk-table-error", "{e}" } },
                    Some(Ok(r)) => rsx! {
                        for generation in [runs()] {
                            ResultGrid { key: "{generation}", ws, owner: node.id, result: SharedResult(r.clone()), context: row_context(), onreload: { let run=run.clone(); move |_| run() } }
                        }
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
fn ResultGrid(
    ws: Workspace,
    owner: moonkale_core::NodeId,
    result: SharedResult,
    context: Option<RowsContext>,
    onreload: EventHandler<()>,
) -> Element {
    let page = use_signal(|| context.as_ref().map(|c| c.page.clone()).unwrap_or_default());
    let undo = use_signal(Vec::new);
    let start = use_signal(|| None);
    let busy = use_signal(|| false);
    let active = use_signal(|| None);
    let mut editing = Editing {
        page,
        pinned: use_signal(Vec::new),
        undo,
        start,
        busy,
        active,
    };
    let current_page = context.as_ref().map(|_| editing.page.read().clone());
    let instance = use_signal(|| moonkale_ext_api::links::panel_instance(ws.content_panel(owner)));
    let pushed = context.as_ref().is_some_and(|c| {
        c.handle.descriptor().capabilities.table_query == moonkale_core::TableCapabilities::SQL
    });
    let editable = context
        .clone()
        .filter(|c| c.handle.descriptor().capabilities.atomic_rows);
    let mut browse = crate::browse::Browse {
        query: use_signal(moonkale_core::TableQuery::default),
        busy: use_signal(|| false),
        error: use_signal(String::new),
        generation: use_signal(|| 0),
        skipped: use_signal(|| 0),
        reposition: use_signal(|| None),
    };
    let mut initialized = use_signal(|| false);
    moonkale_ext_api::crash::use_effect({
        let context = context.clone();
        move || {
            let query = browse.query.read().clone();
            if !*initialized.peek() {
                initialized.set(true);
                return;
            }
            if pushed {
                if let Some(context) = context.clone() {
                    let generation = *browse.generation.peek() + 1;
                    browse.generation.set(generation);
                    browse.busy.set(true);
                    browse.error.set(String::new());
                    moonkale_ext_api::crash::spawn(crate::browse::load(
                        ws, context, editing, browse, query, generation,
                    ));
                }
            }
        }
    });
    let live_table = use_memo({
        let authoritative = context.is_some();
        move || {
            authoritative.then(|| {
                let page = editing.page.read();
                let mut table = crate::browse::table(&page);
                table.truncated |= *browse.skipped.read() > 0
                    || crate::browse::outside_filter(
                        &page,
                        &browse.query.read(),
                        &editing.pinned.read(),
                    );
                table
            })
        }
    });
    moonkale_ext_api::crash::use_effect({
        let result = result.clone();
        move || {
            let table = live_table.read().clone();
            let latest = table
                .map(|table| {
                    Rc::new(QueryResult {
                        table: Some(table),
                        nodes: result.0.nodes.clone(),
                        edges: result.0.edges.clone(),
                        truncated: result.0.truncated,
                    })
                })
                .unwrap_or_else(|| result.0.clone());
            let same = ws
                .links
                .results
                .peek()
                .get(&owner)
                .is_some_and(|r| Rc::ptr_eq(r, &latest) || **r == *latest);
            if !same {
                let mut ws = ws;
                ws.links.results.with_mut(|r| {
                    r.insert(owner, latest);
                });
            }
        }
    });
    let row_ids = use_memo({
        let result = result.clone();
        move || {
            let live = live_table.read();
            let table = live.as_ref().unwrap_or_else(|| result.table());
            table
                .rows
                .iter()
                .enumerate()
                .map(|(i, r)| moonkale_ext_api::result::row_selection(owner, i, r))
                .collect::<Vec<_>>()
        }
    });
    let live_table_guard = live_table.read();
    let table = live_table_guard.as_ref().unwrap_or_else(|| result.table());
    let total = table.rows.len();
    let n_cols = table.columns.len();
    let mut widths = use_signal(|| grid::initial_widths(&table.columns, &table.rows));

    let mut filter = use_signal(String::new);
    let mut scroll = use_signal(|| (0.0f64, 0.0f64));
    // Until the first resize report, a generous guess: overscan covers it.
    let mut view = use_signal(|| (1600.0f64, 900.0f64));
    let mut cursor = use_signal(|| None::<(usize, usize)>);
    // A column resize in progress: (column, pointer x at the start, width).
    let mut drag = use_signal(|| None::<(usize, f64, f64)>);
    let mut grid_el = use_signal(|| None::<Rc<MountedData>>);
    moonkale_ext_api::crash::use_effect(move || {
        let reposition = *browse.reposition.read();
        if let Some(delta) = reposition {
            if let Some(el) = grid_el.peek().clone() {
                let (x, y) = *scroll.peek();
                let top = if delta.is_infinite() {
                    0.0
                } else {
                    (y - delta).max(0.0)
                };
                scroll.set((x, top));
                cursor.set(None);
                browse.reposition.set(None);
                moonkale_ext_api::crash::spawn(async move {
                    let _ = el
                        .scroll(
                            dioxus::html::geometry::PixelsVector2D::new(x, top),
                            ScrollBehavior::Instant,
                        )
                        .await;
                });
            }
        }
    });
    let order = use_memo({
        let r = result.clone();
        let authoritative = context.is_some();
        move || {
            if authoritative {
                let p = editing.page.read();
                let values: Vec<_> = p
                    .rows
                    .iter()
                    .map(|r| p.columns.iter().map(|c| r.values[c].clone()).collect())
                    .collect();
                if pushed {
                    (0..p.rows.len()).collect()
                } else {
                    grid::table_order(&p.columns, &values, &browse.query.read(), &filter.read())
                }
            } else {
                grid::table_order(
                    &r.table().columns,
                    &r.table().rows,
                    &browse.query.read(),
                    &filter.read(),
                )
            }
        }
    });

    moonkale_ext_api::crash::use_effect(move || {
        if let Some(subject) = ws.link_subject(&instance.read()) {
            let has_row = subject.selection.iter().any(|id| id.starts_with("row:"));
            let row = row_ids.read().iter().position(|ids| {
                if has_row {
                    ids.first().is_some_and(|id| subject.selection.contains(id))
                } else {
                    ids.iter().skip(1).any(|id| subject.selection.contains(id))
                }
            });
            if let Some(pos) = row.and_then(|row| order.peek().iter().position(|i| *i == row)) {
                if cursor.peek().is_none_or(|(current, _)| current != pos) {
                    cursor.set(Some((pos, 0)));
                    if let Some(el) = grid_el.peek().clone() {
                        let top = pos as f64 * ROW_H;
                        moonkale_ext_api::crash::spawn(async move {
                            let _ = el
                                .scroll(
                                    dioxus::html::geometry::PixelsVector2D::new(0.0, top),
                                    ScrollBehavior::Instant,
                                )
                                .await;
                        });
                    }
                }
            }
        }
    });
    moonkale_ext_api::crash::use_effect(move || {
        let identities = row_ids.read();
        if browse.reposition.peek().is_some() {
            return;
        }
        if let Some((pos, _)) = *cursor.read() {
            if let Some(ids) = order
                .peek()
                .get(pos)
                .and_then(|i| identities.get(*i).cloned())
            {
                ws.select_linked(&instance.peek(), ids);
            }
        }
    });
    let shown = order.read().len();
    let lefts = grid::lefts(&widths.read());
    let full_w = lefts[n_cols];
    let (sl, st) = scroll();
    let (vw, vh) = view();
    let rows = grid::row_window(st, vh, shown);
    let cols = grid::col_window(&lefts, sl, vw);
    let cur = cursor();
    let sorted = browse.query.read().sort.clone();
    let columns = table.columns.clone();
    let sort_columns = columns.clone();

    let key_result = result.clone();
    let key_context = editable.clone();
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
                moonkale_ext_api::crash::spawn(async move {
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
            if let Some(context) = editable.clone() {
                button { class: "mk-table-undo", disabled: editing.undo.read().is_empty() || *editing.busy.read(), title: t!(ws,L,"table-edit-undo"), onclick: move |_| { let context=context.clone(); moonkale_ext_api::crash::spawn(async move { if let Err(e)=crate::edit::undo(ws,context,editing).await { ws.set_status(e); } }); }, {t!(ws,L,"table-edit-undo")} }
            }
            if shown == total {
                {t!(ws, L, "table-counts", rows = total, columns = n_cols)}
            } else {
                {t!(ws, L, "table-counts-filtered", shown = shown, rows = total, columns = n_cols)}
            }
            if !result.0.nodes.is_empty() { {t!(ws, L, "table-graph-counts", nodes = result.0.nodes.len(), edges = result.0.edges.len())} }
            if table.truncated || result.0.truncated { {t!(ws, L, "table-truncated")} }
            if !pushed { span { class: "mk-table-partial", {t!(ws,L,"table-partial")} }
                details { class: "mk-table-search-menu", summary { title: t!(ws,L,"table-filter"), "⋯" }
            input {
                class: "mk-table-filter",
                r#type: "search",
                placeholder: t!(ws, L, "table-filter"),
                value: "{filter}",
                oninput: moonkale_ext_api::crash::event(move |e: dioxus::prelude::FormEvent| {
                    filter.set(e.value());
                    cursor.set(None);
                }),
            }
                }
            }
            if pushed {
                button { class: "mk-table-next", disabled: *browse.busy.read() || *editing.busy.read() || editing.active.read().is_some() || editing.page.read().next.is_none(), onclick: move |_| { cursor.set(None); browse.next(editing); }, {t!(ws,L,"table-next-page")} }
                button { class: "mk-table-refresh", disabled: *browse.busy.read() || *editing.busy.read() || editing.active.read().is_some(), onclick: move |_| { let q=browse.query.peek().clone(); browse.reset(q); onreload.call(()); }, {t!(ws,L,"table-refresh")} }
                if *browse.busy.read() { span { role: "status", {t!(ws,L,"table-loading")} } }
                if crate::browse::outside_filter(&editing.page.read(), &browse.query.read(), &editing.pinned.read()) { span { class: "mk-table-retained", role: "status", {t!(ws,L,"table-edited-outside")} } }
                if editing.pinned.read().len() >= 1000 { span { role: "status", {t!(ws,L,"table-edit-cap")} } }
            }
            if !browse.error.read().is_empty() { span { role: "alert", "{browse.error}" } }

        }
        div {
            class: "mk-table-grid",
            role: "grid",
            tabindex: "0",
            aria_rowcount: "{shown + 1}",
            aria_colcount: "{n_cols}",
            "data-pushdown": "{pushed}",
            "data-skipped": "{browse.skipped}",
            "data-has-next": "{editing.page.read().next.is_some()}",
            "data-loading": "{browse.busy}",
            "data-generation": "{browse.generation}",
            "data-rows": "{shown}",
            "data-total": "{total}",
            "data-first": "{rows.start}",
            "data-drawn": "{rows.len()}",
            "data-cols-drawn": "{cols.len()}",
            onmounted: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MountedEvent| grid_el.set(Some(e.data()))),
            onresize: moonkale_ext_api::crash::event(move |e: dioxus::prelude::ResizeEvent| {
                if let Ok(size) = e.data().get_content_box_size() {
                    view.set((size.width, size.height));
                }
            }),
            onscroll: moonkale_ext_api::crash::event(move |e: dioxus::prelude::ScrollEvent| {
                let d = e.data();
                scroll.set((d.scroll_left(), d.scroll_top()));
                if pushed && d.scroll_top() + d.client_height() as f64 >= HEAD_H + total as f64 * ROW_H - 2.0 * ROW_H {
                    cursor.set(None);
                    browse.next(editing);
                }
                if d.client_width() > 0 {
                    view.set((d.client_width() as f64, d.client_height() as f64));
                }
            }),
            onkeydown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::KeyboardEvent| {
                let primary = moonkale_ext_api::keys::primary(&e.modifiers());
                let left_hand = ws.settings.resolved.peek().keymap == "left-hand" && !primary && !e.modifiers().alt();
                if e.is_composing() { return; }
                if primary && e.key() == Key::Character("z".into()) {
                    if let Some(context) = key_context.clone() { e.prevent_default(); e.stop_propagation(); moonkale_ext_api::crash::spawn(async move { if let Err(e) = crate::edit::undo(ws, context, editing).await { ws.set_status(e); } }); }
                    return;
                }
                if e.key() == Key::Enter {
                    if key_context.is_some() {
                        if let Some((pos,c)) = *cursor.peek() { if let Some(i) = order.peek().get(pos).copied() { editing.start.set(Some((i,c))); e.prevent_default(); e.stop_propagation(); } }
                        return;
                    }
                    if let Some((row,col)) = *cursor.peek() {
                        let target = order.peek().get(row).and_then(|i| key_result.table().rows.get(*i)).and_then(|r| r.get(col)).and_then(|v| match v { Value::Ref {node,..} => Some(*node), _ => None });
                        if let Some(target) = target { e.prevent_default(); e.stop_propagation(); let new_group = e.modifiers().alt(); moonkale_ext_api::crash::spawn(async move { open_reference(ws,target,None,false,new_group).await; }); }
                    }
                    return;
                }
                let mv = match e.key() {
                    Key::Tab if left_hand && e.modifiers().shift() => Move::Previous,
                    Key::Tab if left_hand => Move::Next,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("w") => Move::Up,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("s") => Move::Down,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("a") => Move::Left,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("d") => Move::Right,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("q") => Move::PageUp,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("e") => Move::PageDown,
                    Key::Character(ref c) if left_hand && c.eq_ignore_ascii_case("f") => { e.prevent_default(); let _ = dioxus::document::eval("const t=document.activeElement?.closest('.mk-table');const i=t?.querySelector('.mk-table-filter');if(i){i.closest('details').open=true;i.focus()}else{t?.querySelector('.mk-column-menu summary')?.focus()}"); return; },
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
                e.stop_propagation();
                go(mv);
            }),
            onmousemove: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| {
                if let Some((c, x0, w0)) = drag() {
                    let x = e.client_coordinates().x;
                    if let Some(w) = widths.write().get_mut(c) {
                        *w = (w0 + x - x0).max(grid::MIN_W);
                    }
                }
            }),
            onmouseup: moonkale_ext_api::crash::event(move |_| drag.set(None)),
            onmouseleave: moonkale_ext_api::crash::event(move |_| drag.set(None)),
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
                            aria_sort: match sorted.iter().find(|s|s.column==columns[c]) { Some(s) if s.descending => "descending", Some(_) => "ascending", _ => "none" },
                            style: "width: {widths.read()[c]}px;",
                            title: "{table.columns[c]}",
                            tabindex: "0",
                            onclick: { let column=sort_columns[c].clone(); move |e: MouseEvent| { if !*browse.busy.peek() && editing.active.peek().is_none() { crate::browse::sort(browse,column.clone(),e.modifiers().shift()); cursor.set(None); } } },
                            onkeydown: { let column=sort_columns[c].clone(); move |e: KeyboardEvent| { e.stop_propagation(); if matches!(e.key(),Key::Enter) || e.key()==Key::Character(" ".into()) { e.prevent_default(); if !*browse.busy.peek() && editing.active.peek().is_none() { crate::browse::sort(browse,column.clone(),e.modifiers().shift()); cursor.set(None); } } } },
                            span { class: "mk-table-hname", "{table.columns[c]}" }
                            span { class: "mk-table-arrow", match sorted.iter().find(|s|s.column==columns[c]) { Some(s) if s.descending => "▼", Some(_) => "▲", _ => "" } }
                            crate::browse::FilterMenu { ws, column: columns[c].clone(), sample: table.rows.iter().filter_map(|r|r.get(c)).find(|v|**v!=Value::Null).cloned().unwrap_or(Value::Null), browse, disabled: *browse.busy.read() || editing.active.read().is_some() }
                            div {
                                class: "mk-table-resize",
                                onclick: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| e.stop_propagation()),
                                onmousedown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| {
                                    e.stop_propagation();
                                    e.prevent_default();
                                    let w = widths.peek().get(c).copied().unwrap_or(grid::MIN_W);
                                    drag.set(Some((c, e.client_coordinates().x, w)));
                                }),
                            }
                        }
                    }
                }
                div { class: "mk-table-body", style: "top: {HEAD_H + rows.start as f64 * ROW_H}px; width: {NUM_W + full_w}px;",
                    for pos in rows.clone() {
                        {
                            let i = order.read()[pos];
                            let row = &table.rows[i];
                            let shared = current_page.as_ref().map(|p|SharedRow(Rc::new(p.rows[i].clone())));
                            rsx! {
                                div { key: "{i}", class: "mk-table-row", role: "row", aria_rowindex: "{pos + 2}", "data-row": "{i}",
                                    div { class: "mk-table-num", "{i + 1 + *browse.skipped.read()}" }
                                    div { class: "mk-table-gap", style: "width: {lefts[cols.start]}px;" }
                                    for c in cols.clone() {
                                        {
                                            let v = shared.as_ref().and_then(|r|r.0.values.get(&table.columns[c])).or_else(||row.get(c)).unwrap_or(&Value::Null);
                                            let text = match v { Value::DateTime(d) => d.display(&ws.lang()), _ => grid::cell_text(v) };
                                            let target = match v { Value::Ref { node, .. } => Some(*node), _ => None };
                                            let known = target.and_then(|id| result.0.nodes.iter().find(|n| n.id == id).cloned().or_else(|| ws.open_node_by_id(id)));
                                            let double_known = known.clone();
                                            let edit_context = editable.clone();
                                            let points = match v { Value::Vector(v) => Some(grid::sparkline(v)), _ => None };
                                            let at = cur == Some((pos, c));
                                            rsx! {
                                                div {
                                                    key: "{c}",
                                                    class: if at { "{cell_class(v)} mk-cell-cursor" } else { "{cell_class(v)}" },
                                                    role: "gridcell",
                                                    style: "width: {widths.read()[c]}px;",
                                                    title: if matches!(v, Value::Text(_)) { "{text}" } else { "" },
                                                    onclick: moonkale_ext_api::crash::event(move |_| cursor.set(Some((pos, c)))),
                                                    ondoubleclick: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| {
                                                        if edit_context.is_some() { editing.start.set(Some((i,c))); return; }
                                                                                                                if let Some(target) = target { let known = double_known.clone(); let new_group = e.modifiers().alt(); moonkale_ext_api::crash::spawn(async move { open_reference(ws,target,known,false,new_group).await; }); }
                                                    }),
                                                    if let Some(context) = editable.clone() {
                                                        crate::edit::Cell { ws, context, snapshot: shared.clone().expect("editable row snapshot"), editing, blocked: *browse.busy.read(), row: i, column: c, onreload,
                                                            onfinish: move |_| { if let Some(el)=grid_el.peek().clone() { moonkale_ext_api::crash::spawn(async move { let _=el.set_focus(true).await; }); } },
                                                            onmove: { let mut move_cursor=go; move |(row,c,movement)| {
                                                                if let Some(pos)=order.peek().iter().position(|i|*i==row) {
                                                                    cursor.set(Some((pos,c)));
                                                                    move_cursor(movement);
                                                                    if let Some((pos,c))=*cursor.peek() { if let Some(row)=order.peek().get(pos).copied() { editing.start.set(Some((row,c))); } }
                                                                } else if let Some(el)=grid_el.peek().clone() {
                                                                    moonkale_ext_api::crash::spawn(async move { let _=el.set_focus(true).await; });
                                                                }
                                                            } },
                                                        }
                                                    } else if let Some(target) = target {
                                                        { let beside = known.clone(); rsx! { button { class: "mk-cell-ref", role: "link", onclick: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| {
                                                            e.stop_propagation();
                                                            let known = known.clone();
                                                            if ws.link_origin(owner).is_some() {
                                                                let panel = ws.links.focused.peek().clone().unwrap_or_else(|| ws.content_panel(owner));
                                                                ws.select_linked(&panel, vec![target.to_string()]);
                                                            } else { moonkale_ext_api::crash::spawn(async move { open_reference(ws, target, known, false, false).await; }); }
                                                        }), onauxclick: moonkale_ext_api::crash::event(move |e: dioxus::prelude::PointerEvent| {
                                                            if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Auxiliary) { e.prevent_default(); e.stop_propagation(); let node = beside.clone(); moonkale_ext_api::crash::spawn(async move { open_reference(ws, target, node, true, false).await; }); }
                                                        }), "{text}" } } }
                                                    } else if let Some(points) = points {
                                                        svg { class: "mk-cell-vector", view_box: "0 0 100 20", width: "100", height: "20",
                                                            role: "img", "aria-label": "{text}",
                                                            polyline { points, fill: "none", stroke: "currentColor", stroke_width: "1.5" }
                                                        }
                                                    } else { "{text}" }
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

/// Resolve a source-known identity through existing readers; never turn labels
/// into paths or URLs. A result can omit its nodes after transport.
async fn open_reference(
    ws: Workspace,
    target: moonkale_core::NodeId,
    known: Option<Node>,
    background: bool,
    new_group: bool,
) {
    let origin = ws.links.focused.peek().clone();
    if let Some(node) = known {
        if background {
            let _ = ws.load_node_guarded(node, || true).await;
        } else {
            if new_group {
                let _ = ws.open_with_context(node).await;
            } else if let Some(panel) = &origin {
                let _ = ws.steer_linked(panel, node).await;
            } else {
                let _ = ws.open_node(node).await;
            }
        }
        return;
    }
    let sources: Vec<_> = ws
        .sources
        .open
        .peek()
        .iter()
        .map(|s| s.descriptor.id.clone())
        .collect();
    for id in sources {
        if let Some(source) = ws.source(&id) {
            if let Ok(result) = source.query(Query::Node(target)).await {
                if let Some(node) = result.nodes.into_iter().find(|n| n.id == target) {
                    if background {
                        let _ = ws.load_node_guarded(node, || true).await;
                    } else {
                        if new_group {
                            let _ = ws.open_with_context(node).await;
                        } else if let Some(panel) = &origin {
                            let _ = ws.steer_linked(panel, node).await;
                        } else {
                            let _ = ws.open_node(node).await;
                        }
                    }
                    return;
                }
            }
        }
    }
}
