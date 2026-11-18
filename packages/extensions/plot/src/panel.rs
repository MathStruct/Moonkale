use crate::{model::*, PlotCommandSignal, L};
use dioxus::prelude::*;
use moonkale_core::{NodeId, QueryResult, SourceId, Table, Value};
use moonkale_ext_api::{t, Workspace};
use std::{collections::BTreeMap, rc::Rc};

const CSS: Asset = asset!("/assets/plot.css");
const W: f64 = 720.0;
const H: f64 = 360.0;

#[derive(Clone)]
struct Shared(Rc<QueryResult>);
impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct View {
    spec: Spec,
    zoom: f64,
    pan: (f64, f64),
}

#[derive(Clone, Copy)]
struct Memory(Signal<BTreeMap<(String, String), View>>);

async fn load_plot_file_source(mut ws: Workspace, doc_node: NodeId, spec: PlotFile) {
    let find_source = |ws: &Workspace| {
        ws.sources
            .open
            .peek()
            .iter()
            .find(|s| {
                (!spec.source.id.is_empty() && s.descriptor.id.as_str() == spec.source.id)
                    || (!spec.source.path.is_empty()
                        && (s
                            .descriptor
                            .id
                            .as_str()
                            .ends_with(&format!("/{}", spec.source.path))
                            || s.descriptor
                                .id
                                .as_str()
                                .ends_with(&format!(":{}", spec.source.path))
                            || s.descriptor.display_name == spec.source.path))
            })
            .cloned()
            .or_else(|| {
                (spec.source.path.is_empty() && spec.source.id.is_empty())
                    .then(|| {
                        ws.sources
                            .open
                            .peek()
                            .iter()
                            .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Sql)
                            .cloned()
                    })
                    .flatten()
            })
    };

    let mut handle = find_source(&ws);
    if handle.is_none() && !spec.source.path.is_empty() {
        // A spec is content: open its source only when the folder's jailed
        // lookup finds it inside the folder (no link leading outside).
        let inside = match folder_source_id(ws).and_then(|id| ws.source(&id)) {
            Some(folder) => crate::spec_source_inside(folder.as_ref(), &spec.source.path).await,
            None => None,
        };
        // Open exactly the path the jail checked.
        if let (Some(rel), Some(root)) = (inside, ws.folder_root()) {
            let full = format!("{root}/{rel}");
            let _ = ws.open_folder(full).await;
            handle = find_source(&ws);
        }
    }
    let Some(handle) = handle else {
        return;
    };
    let source_id = handle.descriptor.id.clone();
    let source = handle.source.clone();
    let filters = PlotFilter::to_core(&spec.query.filters);

    if !spec.source.table.is_empty() && handle.descriptor.family == moonkale_core::SourceFamily::Sql
    {
        if let Ok(page) = source
            .query_rows(
                spec.source.table.clone(),
                moonkale_core::TableQuery {
                    sort: Vec::new(),
                    filters: filters.clone(),
                    limit: 1000,
                    after: None,
                },
            )
            .await
        {
            if page.refusal.is_none() {
                let table = Table {
                    columns: page.columns.clone(),
                    rows: page
                        .rows
                        .iter()
                        .map(|r| page.columns.iter().map(|c| r.values[c].clone()).collect())
                        .collect(),
                    truncated: page.truncated,
                };
                let is_pushed = handle.descriptor.capabilities.table_query
                    == moonkale_core::TableCapabilities::SQL;
                ws.links.pushdown.with_mut(|all| {
                    if is_pushed {
                        all.insert(
                            doc_node,
                            moonkale_ext_api::links::TablePushdown {
                                source: source_id.clone(),
                                table: spec.source.table.clone(),
                                filters,
                            },
                        );
                    } else {
                        all.remove(&doc_node);
                    }
                });
                ws.links.results.with_mut(|all| {
                    all.insert(
                        doc_node,
                        Rc::new(QueryResult {
                            table: Some(table),
                            ..Default::default()
                        }),
                    );
                });
                return;
            }
        }
    }

    if !spec.query.text.is_empty() {
        let dialect = if spec.query.dialect.is_empty() {
            "sql".to_string()
        } else {
            spec.query.dialect.clone()
        };
        if let Ok(res) = source
            .query(moonkale_core::Query::Text {
                dialect,
                text: spec.query.text.clone(),
            })
            .await
        {
            ws.links.results.with_mut(|all| {
                all.insert(doc_node, Rc::new(res));
            });
        }
    }
}

#[component]
pub fn PlotPanel(ws: Workspace, panel: String) -> Element {
    let file_node = panel
        .strip_prefix(crate::EDITOR_PREFIX)
        .and_then(|s| s.parse::<NodeId>().ok());
    let mut loading_source = use_signal(|| false);

    if let Some(doc_node) = file_node {
        let raw_text = ws
            .document(doc_node)
            .map(|d| d.read().text.clone())
            .unwrap_or_default();
        let parsed = PlotFile::parse(&raw_text);
        let data = ws.links.results.read().get(&doc_node).cloned();
        moonkale_ext_api::crash::use_effect(move || {
            let _open_sources = ws.sources.open.read().len();
            let text = ws
                .document(doc_node)
                .map(|d| d.read().text.clone())
                .unwrap_or_default();
            if let Ok(spec) = PlotFile::parse(&text) {
                if !ws.links.results.peek().contains_key(&doc_node) && !*loading_source.peek() {
                    loading_source.set(true);
                    moonkale_ext_api::crash::spawn(async move {
                        load_plot_file_source(ws, doc_node, spec).await;
                        loading_source.set(false);
                    });
                }
            }
        });
        return rsx! {
            moonkale_ext_api::Stylesheet { href: CSS }
            match parsed {
                Err(err) => rsx! {
                    div { class: "mk-plot", "data-plot-error": "{err}",
                        p { class: "mk-plot-empty", "{err}" }
                    }
                },
                Ok(plot_file) => match data {
                    Some(data) if data.table.is_some() => {
                        let cols = &data.table.as_ref().unwrap().columns;
                        rsx! {
                            PlotData {
                                key: "{doc_node}:{cols:?}",
                                ws,
                                panel,
                                owner: doc_node,
                                file_node: Some(doc_node),
                                initial_plot_file: Some(plot_file),
                                data: Shared(data),
                            }
                        }
                    }
                    _ => rsx! { p { {t!(ws, L, "plot-empty")} } },
                },
            }
        };
    }

    let owner = ws.link_subject(&panel).map(|s| s.node);
    let data = owner.and_then(|o| ws.links.results.read().get(&o).cloned().map(|r| (o, r)));
    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        if let Some((owner, data)) = data {
            if let Some(table) = &data.table {
                PlotData {
                    key: "{owner}:{table.columns:?}",
                    ws,
                    panel,
                    owner,
                    file_node: None,
                    initial_plot_file: None,
                    data: Shared(data.clone()),
                }
            } else {
                p { {t!(ws, L, "plot-empty")} }
            }
        } else {
            p { {t!(ws, L, "plot-empty")} }
        }
    }
}

fn tooltip(result: &QueryResult, row: usize) -> String {
    let Some(table) = result.table.as_ref() else {
        return String::new();
    };
    let Some(values) = table.rows.get(row) else {
        return String::new();
    };
    table
        .columns
        .iter()
        .zip(values)
        .take(32)
        .map(|(c, v)| {
            let value = match v {
                Value::Text(s) | Value::Ref { label: s, .. } => s.chars().take(120).collect(),
                Value::Vector(v) => format!(
                    "{:?} … ({})",
                    &v.as_slice()[..v.as_slice().len().min(8)],
                    v.as_slice().len()
                ),
                _ => v.to_string().chars().take(120).collect::<String>(),
            };
            format!("{}: {value}", c.chars().take(80).collect::<String>())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn relative_source_path(ws: Workspace, source_id: &SourceId) -> String {
    let raw = source_id
        .as_str()
        .split_once(':')
        .map(|(_, p)| p)
        .unwrap_or(source_id.as_str())
        .trim_end_matches('/');
    if let Some(root) = ws.folder_root() {
        let prefix = format!("{}/", root.trim_end_matches('/'));
        if let Some(rel) = raw.strip_prefix(&prefix) {
            return rel.to_string();
        }
    }
    String::new()
}

fn folder_source_id(ws: Workspace) -> Option<SourceId> {
    ws.sources
        .open
        .peek()
        .iter()
        .find(|s| s.descriptor.family == moonkale_core::SourceFamily::Folder)
        .map(|s| s.descriptor.id.clone())
}

fn default_slug(ws: Workspace, owner: NodeId, existing: Option<&PlotFile>) -> String {
    let raw = ws
        .links
        .pushdown
        .peek()
        .get(&owner)
        .map(|p| p.table.clone())
        .or_else(|| {
            existing.and_then(|pf| {
                (!pf.source.table.is_empty())
                    .then(|| pf.source.table.clone())
                    .or_else(|| (!pf.labels.title.is_empty()).then(|| pf.labels.title.clone()))
            })
        })
        .or_else(|| ws.open_node_by_id(owner).map(|n| n.label))
        .unwrap_or_else(|| "plot".into());
    let slug: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "plot".into()
    } else {
        slug
    }
}

fn build_plot_file(
    ws: Workspace,
    owner: NodeId,
    table: &Table,
    s: Spec,
    existing: Option<&PlotFile>,
    default_y_count: &str,
) -> PlotFile {
    let (source, query) = if let Some(pd) = ws.links.pushdown.peek().get(&owner) {
        let rel = relative_source_path(ws, &pd.source);
        (
            PlotSource {
                id: if rel.is_empty() {
                    pd.source.as_str().to_string()
                } else {
                    String::new()
                },
                path: rel,
                table: pd.table.clone(),
            },
            PlotQuery {
                dialect: "sql".into(),
                text: format!(
                    "SELECT * FROM \"{}\" LIMIT 1000",
                    pd.table.replace('"', "\"\"")
                ),
                filters: PlotFilter::from_core(&pd.filters),
            },
        )
    } else if let Some(ex) = existing {
        (ex.source.clone(), ex.query.clone())
    } else if let Some(node) = ws.open_node_by_id(owner) {
        let tbl = node
            .native_key
            .strip_prefix("table:")
            .unwrap_or(&node.native_key)
            .to_string();
        let rel = relative_source_path(ws, &node.source);
        (
            PlotSource {
                id: if rel.is_empty() {
                    node.source.as_str().to_string()
                } else {
                    String::new()
                },
                path: rel,
                table: tbl.clone(),
            },
            PlotQuery {
                dialect: "sql".into(),
                text: format!("SELECT * FROM \"{}\" LIMIT 1000", tbl.replace('"', "\"\"")),
                filters: Vec::new(),
            },
        )
    } else {
        (
            PlotSource {
                path: String::new(),
                table: "result".into(),
                id: String::new(),
            },
            PlotQuery::default(),
        )
    };
    let x_col = table.columns.get(s.x).cloned().unwrap_or_default();
    let y_col = table.columns.get(s.y).cloned().unwrap_or_default();
    let title = existing
        .and_then(|ex| (!ex.labels.title.is_empty()).then(|| ex.labels.title.clone()))
        .or_else(|| (!source.table.is_empty()).then(|| source.table.clone()))
        .unwrap_or_else(|| "Plot".into());
    PlotFile {
        version: PLOT_SPEC_VERSION,
        source,
        query,
        mark: s.mark,
        encodings: PlotEncodings {
            x: x_col.clone(),
            y: y_col.clone(),
        },
        scales: PlotScales {
            x: s.xs,
            y: s.ys,
            equal: s.equal,
        },
        labels: PlotLabels {
            title,
            x: x_col,
            y: if s.mark == Mark::Histogram {
                default_y_count.to_string()
            } else {
                y_col
            },
        },
    }
}

type AggCacheEntry = (
    SourceId,
    String,
    moonkale_core::TableAggregate,
    Result<(moonkale_core::TableAggregateResult, u64), String>,
);

#[component]
fn PlotData(
    ws: Workspace,
    panel: String,
    owner: NodeId,
    file_node: Option<NodeId>,
    initial_plot_file: Option<PlotFile>,
    data: Shared,
) -> Element {
    let table = data.0.table.as_ref().expect("table result");
    let memory = use_hook(|| {
        ws.shared_state(|| Memory(Signal::new_in_scope(BTreeMap::new(), ScopeId::ROOT)))
    });
    let memory_key = (
        ws.saved_sessions.active.peek().clone(),
        format!(
            "{panel}:{owner}:{}",
            NodeId::from_content(format!("{:?}", table.columns).as_bytes())
        ),
    );
    let initial_spec = initial_plot_file
        .as_ref()
        .map(|pf| pf.to_spec(table))
        .unwrap_or_else(|| default_spec(table));
    let mut view = use_signal(|| {
        memory.0.peek().get(&memory_key).copied().unwrap_or(View {
            spec: initial_spec,
            zoom: 1.0,
            pan: (0.0, 0.0),
        })
    });
    use_effect({
        let key = memory_key.clone();
        move || {
            let current = *view.read();
            let mut memory = memory;
            if memory.0.peek().get(&key) != Some(&current) {
                memory.0.with_mut(|all| {
                    if all.len() >= 512 && !all.contains_key(&key) {
                        all.clear();
                    }
                    all.insert(key.clone(), current);
                });
            }
        }
    });

    // Keep an opened `.plot.json` document buffer synced when controls change.
    let sync_initial = initial_plot_file.clone();
    let sync_data = data.clone();
    moonkale_ext_api::crash::use_effect(move || {
        let s = view.read().spec;
        if s == initial_spec {
            return;
        }
        if let Some(doc_node) = file_node {
            if let Some(mut doc) = ws.document(doc_node) {
                if let Some(t) = sync_data.0.table.as_ref() {
                    let count_lbl = t!(ws, L, "plot-count");
                    let next = build_plot_file(ws, owner, t, s, sync_initial.as_ref(), &count_lbl);
                    if let Ok(json) = next.to_json() {
                        if doc.peek().text != json {
                            doc.with_mut(|d| d.text = json);
                        }
                    }
                }
            }
        }
    });

    let mut held = use_signal(|| data.clone());
    if *held.peek() != data {
        held.set(data.clone());
    }
    let spec = use_memo(move || view.read().spec);

    // PLT-11: Pushdown database aggregation for histograms and bars.
    let mut agg_cache = use_signal(|| None::<AggCacheEntry>);
    let mut agg_inflight =
        use_signal(|| None::<(SourceId, String, moonkale_core::TableAggregate, u64)>);
    // PLT-09: the source's data epoch the cached aggregate was computed at. A
    // newer epoch (the file changed outside Moonkale) asks again, and the
    // cached aggregate stays on screen until the new one arrives.
    let mut agg_epoch = use_signal(|| 0u64);
    let agg_request = use_memo(move || {
        let current = held.read();
        let table = current.0.table.as_ref()?;
        let pd = ws.links.pushdown.read().get(&owner).cloned()?;
        let s_now = spec();
        let x_col = table.columns.get(s_now.x)?.clone();
        match s_now.mark {
            Mark::Histogram if s_now.xs != Scale::Time => {
                let is_time = table
                    .rows
                    .iter()
                    .take(16)
                    .filter_map(|r| r.get(s_now.x))
                    .any(|v| matches!(v, Value::DateTime(_)));
                (!is_time).then_some((
                    pd.source.clone(),
                    pd.table.clone(),
                    moonkale_core::TableAggregate {
                        x: x_col,
                        kind: moonkale_core::TableAggregateKind::Histogram {
                            log: s_now.xs == Scale::Log,
                        },
                        filters: pd.filters.clone(),
                    },
                ))
            }
            Mark::Bar => {
                let y_col = table.columns.get(s_now.y)?.clone();
                let is_time = table
                    .rows
                    .iter()
                    .take(16)
                    .filter_map(|r| r.get(s_now.y))
                    .any(|v| matches!(v, Value::DateTime(_)));
                (!is_time).then_some((
                    pd.source.clone(),
                    pd.table.clone(),
                    moonkale_core::TableAggregate {
                        x: x_col,
                        kind: moonkale_core::TableAggregateKind::Bar { y: y_col },
                        filters: pd.filters.clone(),
                    },
                ))
            }
            _ => None,
        }
    });

    moonkale_ext_api::crash::use_effect(move || {
        let _open_sources = ws.sources.open.read().len();
        if let Some((src_id, tbl, req)) = agg_request.read().clone() {
            let epoch = ws
                .sources
                .data_epochs
                .read()
                .get(&src_id)
                .copied()
                .unwrap_or(0);
            let already = (*agg_epoch.peek() == epoch
                && agg_cache
                    .peek()
                    .as_ref()
                    .is_some_and(|(s, t, r, _)| s == &src_id && t == &tbl && r == &req))
                || agg_inflight.peek().as_ref().is_some_and(|(s, t, r, e)| {
                    s == &src_id && t == &tbl && r == &req && *e == epoch
                });
            if !already {
                if let Some(source) = ws.source(&src_id) {
                    agg_inflight.set(Some((src_id.clone(), tbl.clone(), req.clone(), epoch)));
                    moonkale_ext_api::crash::spawn(async move {
                        let t0 = moonkale_ext_api::workspace::now_ms();
                        let res = source
                            .aggregate_rows(tbl.clone(), req.clone())
                            .await
                            .map(|r| (r, moonkale_ext_api::workspace::now_ms().saturating_sub(t0)))
                            .map_err(|e| e.to_string());
                        // A newer request (other columns, a newer epoch) was
                        // sent meanwhile: this answer is outdated.
                        let current = agg_inflight.peek().as_ref().is_some_and(|(s, t, r, e)| {
                            s == &src_id && t == &tbl && r == &req && *e == epoch
                        });
                        if !current {
                            return;
                        }
                        agg_cache.set(Some((src_id, tbl, req, res)));
                        agg_epoch.set(epoch);
                        agg_inflight.set(None);
                    });
                }
            }
        }
    });

    let agg_req_val = agg_request.read().clone();
    let agg_match = agg_req_val.as_ref().and_then(|(src_id, tbl, req)| {
        agg_cache
            .read()
            .as_ref()
            .filter(|(s, t, r, _)| s == src_id && t == tbl && r == req)
            .map(|(_, _, _, res)| res.clone())
    });
    let agg_pending = agg_req_val.is_some() && agg_match.is_none();

    let prepared = use_memo(move || {
        let current = held.read();
        let table = current.0.table.as_ref().unwrap();
        let s = spec();
        if let Some(Ok((agg_res, _))) =
            agg_cache.read().as_ref().and_then(|(src, tbl, req, res)| {
                let pd = ws.links.pushdown.read().get(&owner).cloned()?;
                let x_col = table.columns.get(s.x)?;
                let matches_mark = match (&req.kind, s.mark) {
                    (moonkale_core::TableAggregateKind::Histogram { log }, Mark::Histogram) => {
                        s.xs != Scale::Time && *log == (s.xs == Scale::Log)
                    }
                    (moonkale_core::TableAggregateKind::Bar { y }, Mark::Bar) => {
                        table.columns.get(s.y).is_some_and(|yc| yc == y)
                    }
                    _ => false,
                };
                (src == &pd.source
                    && tbl == &pd.table
                    && &req.x == x_col
                    && req.filters == pd.filters
                    && matches_mark)
                    .then_some(res)
            })
        {
            return prepare_aggregate(table, s, agg_res);
        }
        prepare(table, s)
    });
    // A table-selected row remains visible even when uniform sampling omitted it.
    let base_prepared = prepared;
    let selection_panel = panel.clone();
    let prepared = use_memo(move || {
        let current = held.read();
        let table = current.0.table.as_ref().unwrap();
        let s = spec();
        let base = base_prepared.read();
        let mut out = base.clone();
        if base.sampled && matches!(s.mark, Mark::Line | Mark::Scatter) {
            let selection = ws
                .link_subject(&selection_panel)
                .map(|s| s.selection)
                .unwrap_or_default();
            if let Some(row) = moonkale_ext_api::result::selected_row(owner, table, &selection) {
                if !out.points.iter().any(|p| p.row == row) {
                    if let Some((x, y)) = table.rows[row]
                        .get(s.x)
                        .and_then(number)
                        .zip(table.rows[row].get(s.y).and_then(number))
                    {
                        if s.xs.transform(x).is_some() && s.ys.transform(y).is_some() {
                            if out.points.len() >= POINT_CAP {
                                out.points.remove(out.points.len() / 2);
                            }
                            out.points.push(Point {
                                x,
                                y,
                                row,
                                members: vec![row],
                                count: 1,
                                label: String::new(),
                            });
                            if s.mark == Mark::Line {
                                out.points.sort_by(|a, b| a.x.total_cmp(&b.x));
                            }
                        }
                    }
                }
            }
        }
        out
    });
    let mut hovered = use_signal(|| None::<usize>);
    let mut active = use_signal(|| 0usize);
    let mut moved = use_signal(|| false);
    let mut drag = use_signal(|| None::<(f64, f64, (f64, f64))>);
    let mut size = use_signal(|| (900.0_f64, 500.0_f64));

    let initial_path = use_hook(|| {
        file_node
            .and_then(|n| ws.document(n).map(|d| d.peek().node.native_key.clone()))
            .unwrap_or_else(|| {
                let slug = default_slug(ws, owner, initial_plot_file.as_ref());
                format!(".moonkale/plots/{slug}{PLOT_SPEC_EXTENSION}")
            })
    });
    let mut path_input = use_signal(|| initial_path.clone());
    let mut saved_plot_path = use_signal(|| {
        file_node
            .and_then(|n| ws.document(n).map(|d| d.peek().node.native_key.clone()))
            .unwrap_or_default()
    });
    let mut exported_svg_path = use_signal(String::new);

    let v = view();
    let s = v.spec;
    let prepared_signal = prepared;
    let prepared = prepared.read();
    let points = &prepared.points;
    let selections_memo = use_memo(move || {
        let held = held.read();
        let table = held.0.table.as_ref().unwrap();
        prepared_signal
            .read()
            .points
            .iter()
            .map(|p| {
                let select = table
                    .rows
                    .get(p.row)
                    .map(|r| moonkale_ext_api::result::row_selection(owner, p.row, r))
                    .unwrap_or_default();
                let members = p
                    .members
                    .iter()
                    .filter_map(|i| {
                        table
                            .rows
                            .get(*i)
                            .map(|r| moonkale_ext_api::result::row_selection(owner, *i, r))
                    })
                    .flatten()
                    .collect::<Vec<_>>();
                (select, members)
            })
            .collect::<Vec<_>>()
    });
    let links = selections_memo.read();
    let selections = links
        .iter()
        .map(|(select, _)| select.clone())
        .collect::<Vec<_>>();
    let selected = ws
        .link_subject(&panel)
        .map(|s| s.selection)
        .unwrap_or_default();
    let has_row = selected.iter().any(|id| id.starts_with("row:"));
    let highlighted = links
        .iter()
        .map(|(_, members)| {
            members
                .iter()
                .any(|id| (!has_row || id.starts_with("row:")) && selected.contains(id))
        })
        .collect::<Vec<_>>();
    let xs = if matches!(s.mark, Mark::Bar | Mark::Histogram) {
        Scale::Linear
    } else {
        s.xs
    };
    let ys = s.ys;
    let mut xa = Axis::from_values(points.iter().filter_map(|p| xs.transform(p.x)));
    if matches!(s.mark, Mark::Bar | Mark::Histogram) {
        let half = if s.mark == Mark::Bar {
            0.5
        } else {
            prepared.bar_width / 1.8
        };
        xa = Axis::from_values(points.iter().flat_map(|p| [p.x - half, p.x + half]));
    }
    let mut ya = Axis::from_values(points.iter().filter_map(|p| ys.transform(p.y)).chain(
        (matches!(s.mark, Mark::Bar | Mark::Histogram) && ys == Scale::Linear).then_some(0.0),
    ));
    if matches!(s.mark, Mark::Line | Mark::Scatter) {
        let px = (xa.hi - xa.lo) * 0.04;
        let py = (ya.hi - ya.lo) * 0.04;
        if (xa.lo - px).is_finite() && (xa.hi + px).is_finite() {
            xa.lo -= px;
            xa.hi += px;
        }
        if (ya.lo - py).is_finite() && (ya.hi + py).is_finite() {
            ya.lo -= py;
            ya.hi += py;
        }
    }
    if s.equal && ((ya.hi - ya.lo) * W / H).is_finite() {
        let span = (xa.hi - xa.lo).max((ya.hi - ya.lo) * W / H);
        let mx = xa.lo + (xa.hi - xa.lo) / 2.0;
        let my = ya.lo + (ya.hi - ya.lo) / 2.0;
        xa = Axis {
            lo: mx - span / 2.0,
            hi: mx + span / 2.0,
        };
        ya = Axis {
            lo: my - span * H / W / 2.0,
            hi: my + span * H / W / 2.0,
        };
    }
    xa = xa.viewport(v.pan.0, v.zoom);
    ya = ya.viewport(v.pan.1, v.zoom);
    let coords: Vec<_> = points
        .iter()
        .map(|p| {
            xs.transform(p.x)
                .zip(ys.transform(p.y))
                .map(|(x, y)| (100.0 + W * xa.unit(x), 410.0 - H * ya.unit(y)))
        })
        .collect();
    let polyline = coords
        .iter()
        .flatten()
        .map(|(x, y)| format!("{x:.3},{y:.3}"))
        .collect::<Vec<_>>()
        .join(" ");

    let agg_used = match &agg_match {
        Some(Ok((agg_res, elapsed_ms))) => Some((agg_res.total_rows as usize, *elapsed_ms)),
        _ => None,
    };
    let partial = if agg_used.is_some() {
        false
    } else {
        table.truncated || data.0.truncated || table.rows.len() > INPUT_CAP
    };
    let total_rows = agg_used
        .map(|(rows, _)| rows)
        .unwrap_or_else(|| table.rows.len());
    let agg_ms_attr = agg_used.map(|(_, ms)| ms.to_string()).unwrap_or_default();
    let mark_attr = if agg_pending { "" } else { s.mark.id() };

    let selected_point = highlighted.iter().position(|b| *b);
    let info = hovered()
        .or(selected_point)
        .and_then(|i| points.get(i))
        .map(|p| {
            let text = tooltip(&data.0, p.row);
            if p.count > 1 {
                format!(
                    "{}\n{}\n{text}",
                    p.label,
                    t!(ws, L, "plot-aggregate", count = p.count)
                )
            } else {
                text
            }
        })
        .unwrap_or_default();
    let x_label = table.columns.get(s.x).cloned().unwrap_or_default();
    let y_label = if s.mark == Mark::Histogram {
        t!(ws, L, "plot-count")
    } else {
        table.columns.get(s.y).cloned().unwrap_or_default()
    };
    let clip = format!("plot-clip-{}", NodeId::from_content(panel.as_bytes()));
    let svg_scale = (size().0 / 900.0).min(size().1 / 500.0).max(0.01);
    let tick_font = 11.0 / svg_scale;
    let label_font = 14.0 / svg_scale;
    let x_label_cap = (W * svg_scale / 14.0).clamp(8.0, 50.0) as usize;
    let y_label_cap = (H * svg_scale / 14.0).clamp(8.0, 24.0) as usize;

    let x_ticks = if s.mark == Mark::Bar {
        let cap = (W * svg_scale / 140.0).floor().clamp(2.0, 8.0) as usize;
        decimate(prepared.categories.len(), cap)
            .into_iter()
            .map(|i| {
                (
                    i as f64,
                    prepared.categories[i].chars().take(10).collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
    } else {
        xa.ticks(
            W * svg_scale,
            if s.xs == Scale::Time { 190.0 } else { 100.0 },
        )
        .into_iter()
        .map(|tick| {
            (
                tick,
                if s.mark == Mark::Histogram {
                    s.xs.label(tick)
                } else {
                    xs.label(tick)
                },
            )
        })
        .collect()
    };

    let do_save = {
        let panel = panel.clone();
        let initial_plot_file = initial_plot_file.clone();
        move || {
            let mut ws = ws;
            let current = held.peek().clone();
            let Some(table) = current.0.table.as_ref() else {
                return;
            };
            let s = view.peek().spec;
            let count_lbl = t!(ws, L, "plot-count");
            let plot_file =
                build_plot_file(ws, owner, table, s, initial_plot_file.as_ref(), &count_lbl);
            let Ok(json) = plot_file.to_json() else {
                return;
            };
            let raw_path = path_input.peek().trim().trim_start_matches('/').to_string();
            let target_path = if raw_path.is_empty() {
                let slug = default_slug(ws, owner, initial_plot_file.as_ref());
                format!(".moonkale/plots/{slug}{PLOT_SPEC_EXTENSION}")
            } else if raw_path.ends_with(PLOT_SPEC_EXTENSION) {
                raw_path
            } else {
                format!("{raw_path}{PLOT_SPEC_EXTENSION}")
            };
            path_input.set(target_path.clone());
            let panel = panel.clone();
            moonkale_ext_api::crash::spawn(async move {
                if let Some(doc_node) = file_node {
                    if let Some(mut doc) = ws.document(doc_node) {
                        if doc.peek().node.native_key == target_path {
                            doc.with_mut(|d| d.text = json);
                            if ws.save(doc_node).await.is_ok() {
                                saved_plot_path.set(target_path.clone());
                                ws.set_status(t!(ws, L, "plot-saved", path = target_path));
                            }
                            return;
                        }
                    }
                }
                let Some(folder_id) = folder_source_id(ws) else {
                    return;
                };
                if ws
                    .write_text_at(&folder_id, &target_path, &json)
                    .await
                    .is_ok()
                {
                    saved_plot_path.set(target_path.clone());
                    ws.set_status(t!(ws, L, "plot-saved", path = target_path.clone()));
                    if let Some(node) = ws.node_at_path(&folder_id, &target_path).await {
                        let new_id = node.id;
                        let existing_res = ws.links.results.peek().get(&owner).cloned();
                        if let Some(res) = existing_res {
                            ws.links.results.with_mut(|all| {
                                all.insert(new_id, res);
                            });
                        }
                        let existing_pd = ws.links.pushdown.peek().get(&owner).cloned();
                        if let Some(pd) = existing_pd {
                            ws.links.pushdown.with_mut(|all| {
                                all.insert(new_id, pd);
                            });
                        }
                        let _ = ws.open_node(node).await;
                        if panel.starts_with("plot:") {
                            ws.close_linked_panel(&panel);
                            ws.links.views.with_mut(|v| {
                                v.remove(&panel);
                            });
                        }
                    }
                }
            });
        }
    };

    let do_export_svg = {
        let initial_plot_file = initial_plot_file.clone();
        move || {
            let mut ws = ws;
            let current = held.peek().clone();
            let Some(table) = current.0.table.as_ref() else {
                return;
            };
            let s = view.peek().spec;
            let prep = prepared_signal.peek().clone();
            let count_lbl = t!(ws, L, "plot-count");
            let plot_file =
                build_plot_file(ws, owner, table, s, initial_plot_file.as_ref(), &count_lbl);
            let svg = render_svg(table, s, &prep, &plot_file.labels, &count_lbl);
            let raw_path = path_input.peek().trim().trim_start_matches('/').to_string();
            let svg_path = if raw_path.ends_with(".svg") {
                raw_path
            } else if let Some(stem) = raw_path.strip_suffix(PLOT_SPEC_EXTENSION) {
                format!("{stem}.svg")
            } else if let Some(stem) = raw_path.strip_suffix(".json") {
                format!("{stem}.svg")
            } else if raw_path.is_empty() {
                let slug = default_slug(ws, owner, initial_plot_file.as_ref());
                format!(".moonkale/plots/{slug}.svg")
            } else {
                format!("{raw_path}.svg")
            };
            moonkale_ext_api::crash::spawn(async move {
                let Some(folder_id) = folder_source_id(ws) else {
                    return;
                };
                if ws.write_text_at(&folder_id, &svg_path, &svg).await.is_ok() {
                    exported_svg_path.set(svg_path.clone());
                    ws.set_status(t!(ws, L, "plot-exported-svg", path = svg_path));
                }
            });
        }
    };

    // Palette command listener (`plot.save` / `plot.export_svg`).
    let cmd_sig = use_hook(|| {
        ws.shared_state(|| PlotCommandSignal(Signal::new_in_scope((0, ""), ScopeId::ROOT)))
            .0
    });
    let mut last_cmd_seq = use_signal(|| cmd_sig.peek().0);
    let mut save_from_cmd = do_save.clone();
    let export_from_cmd = do_export_svg.clone();
    moonkale_ext_api::crash::use_effect(move || {
        let (seq, cmd) = *cmd_sig.read();
        if seq > *last_cmd_seq.peek() {
            last_cmd_seq.set(seq);
            match cmd {
                "plot.save" => save_from_cmd(),
                "plot.export_svg" => export_from_cmd(),
                _ => {}
            }
        }
    });

    let mut save_btn = do_save;
    let export_btn = do_export_svg;
    let key_panel = panel.clone();
    let key_selections = selections.to_vec();
    let n = points.len();
    rsx! {
        div {
            class: "mk-plot",
            "data-mark": mark_attr,
            "data-points": "{n}",
            "data-partial": "{partial}",
            "data-total-rows": "{total_rows}",
            "data-agg-ms": "{agg_ms_attr}",
            "data-plot-path": "{saved_plot_path}",
            "data-exported-svg": "{exported_svg_path}",
            "data-zoom": "{v.zoom}",
            div { class: "mk-plot-toolbar",
                moonkale_ext_api::icons::ToolbarActions {
                    primary: rsx! {
                        button {
                            class: "mk-btn mk-plot-fit",
                            onclick: move |_| view.with_mut(|v| { v.zoom = 1.0; v.pan = (0.0, 0.0); }),
                            {t!(ws, L, "plot-fit")}
                        }
                    },
                    label {
                        {t!(ws, L, "plot-x")}
                        select {
                            class: "mk-plot-x",
                            value: "{s.x}",
                            onchange: move |e| {
                                if let Ok(x) = e.value().parse() {
                                    view.with_mut(|v| v.spec.x = x);
                                }
                            },
                            for (i, c) in table.columns.iter().take(256).enumerate() {
                                option { key: "{i}", value: "{i}", "{c}" }
                            }
                        }
                    }
                    label {
                        {t!(ws, L, "plot-y")}
                        select {
                            class: "mk-plot-y",
                            value: "{s.y}",
                            disabled: s.mark == Mark::Histogram,
                            onchange: move |e| {
                                if let Ok(y) = e.value().parse() {
                                    view.with_mut(|v| v.spec.y = y);
                                }
                            },
                            for (i, c) in table.columns.iter().take(256).enumerate() {
                                option { key: "{i}", value: "{i}", "{c}" }
                            }
                        }
                    }
                    label {
                        {t!(ws, L, "plot-mark")}
                        select {
                            class: "mk-plot-mark",
                            value: s.mark.id(),
                            onchange: move |e| {
                                if let Some(m) = Mark::ALL.into_iter().find(|m| m.id() == e.value()) {
                                    view.with_mut(|v| v.spec.mark = m);
                                }
                            },
                            for m in Mark::ALL {
                                option { value: m.id(), {mark_name(ws, m)} }
                            }
                        }
                    }
                    for (is_x, scale, label) in [(true, s.xs, t!(ws, L, "plot-x-scale")), (false, s.ys, t!(ws, L, "plot-y-scale"))] {
                        label {
                            "{label}"
                            select {
                                class: if is_x { "mk-plot-xs" } else { "mk-plot-ys" },
                                value: scale.id(),
                                disabled: is_x && s.mark == Mark::Bar,
                                onchange: move |e| {
                                    if let Some(scale) = Scale::ALL.into_iter().find(|s| s.id() == e.value()) {
                                        view.with_mut(|v| {
                                            if is_x {
                                                v.spec.xs = scale;
                                            } else {
                                                v.spec.ys = scale;
                                            }
                                        });
                                    }
                                },
                                for scale in Scale::ALL {
                                    option { value: scale.id(), {scale_name(ws, scale)} }
                                }
                            }
                        }
                    }
                    label {
                        input {
                            class: "mk-plot-equal",
                            r#type: "checkbox",
                            checked: s.equal,
                            onchange: move |e| view.with_mut(|v| v.spec.equal = e.checked()),
                        }
                        {t!(ws, L, "plot-equal")}
                    }
                    for (factor, label) in [(1.5, t!(ws, L, "plot-zoom-in")), (1.0 / 1.5, t!(ws, L, "plot-zoom-out"))] {
                        button {
                            class: "mk-btn",
                            onclick: move |_| view.with_mut(|v| v.zoom = (v.zoom * factor).clamp(0.25, 1000.0)),
                            "{label}"
                        }
                    }
                    for (x, y, label) in [(-0.15, 0.0, t!(ws, L, "plot-left")), (0.15, 0.0, t!(ws, L, "plot-right")), (0.0, 0.15, t!(ws, L, "plot-up")), (0.0, -0.15, t!(ws, L, "plot-down"))] {
                        button {
                            class: "mk-btn",
                            onclick: move |_| {
                                view.with_mut(|v| {
                                    v.pan.0 = (v.pan.0 + x / v.zoom).clamp(-10.0, 10.0);
                                    v.pan.1 = (v.pan.1 + y / v.zoom).clamp(-10.0, 10.0);
                                })
                            },
                            "{label}"
                        }
                    }
                    label {
                        {t!(ws, L, "plot-path")}
                        input {
                            class: "mk-input mk-plot-path",
                            r#type: "text",
                            value: "{path_input}",
                            oninput: move |e| path_input.set(e.value()),
                        }
                    }
                    button {
                        class: "mk-btn mk-plot-save",
                        r#type: "button",
                        onclick: move |_| save_btn(),
                        {t!(ws, L, "plot-save")}
                    }
                    button {
                        class: "mk-btn mk-plot-export-svg",
                        r#type: "button",
                        onclick: move |_| export_btn(),
                        {t!(ws, L, "plot-export-svg")}
                    }
                }
            }
            p { class: "mk-plot-summary", role: "status",
                if let Some((rows, ms)) = agg_used {
                    {t!(ws, L, "plot-aggregated", rows = rows, marks = n, ms = ms)}
                } else {
                    {t!(ws, L, "plot-loaded", rows = table.rows.len(), marks = n)}
                }
                if partial {
                    span { class: "mk-plot-partial", {t!(ws, L, "plot-partial")} }
                }
                if prepared.sampled {
                    span { class: "mk-plot-sampled", {t!(ws, L, "plot-sampled", cap = POINT_CAP)} }
                }
                if prepared.omitted > 0 {
                    span { {t!(ws, L, "plot-omitted", count = prepared.omitted)} }
                }
            }
            p { class: "mk-plot-help", {t!(ws, L, "plot-help")} }
            svg {
                id: "plot-canvas-{panel}",
                class: "mk-plot-canvas",
                view_box: "0 0 900 500",
                preserve_aspect_ratio: "xMidYMid meet",
                tabindex: "0",
                role: "group",
                "aria-label": t!(ws, L, "plot-title"),
                onmounted: move |e| {
                    let el = e.data();
                    moonkale_ext_api::crash::spawn(async move {
                        let _ = el.set_focus(true).await;
                    });
                },
                onresize: move |e| {
                    let z = e.get_content_box_size();
                    if let Ok(z) = z {
                        size.set((z.width.max(1.0), z.height.max(1.0)));
                    }
                },
                onwheel: move |e: WheelEvent| {
                    e.prevent_default();
                    let factor = if e.delta().strip_units().y < 0.0 { 1.2 } else { 1.0 / 1.2 };
                    view.with_mut(|v| v.zoom = (v.zoom * factor).clamp(0.25, 1000.0));
                },
                onpointerdown: move |e: PointerEvent| {
                    if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                        let c = e.client_coordinates();
                        moved.set(false);
                        drag.set(Some((c.x, c.y, view.peek().pan)));
                    }
                },
                onpointermove: move |e: PointerEvent| {
                    if let Some((x, y, pan)) = *drag.peek() {
                        let c = e.client_coordinates();
                        if (c.x - x).abs() + (c.y - y).abs() > 3.0 {
                            moved.set(true);
                        }
                        let (width, height) = *size.peek();
                        view.with_mut(|v| {
                            v.pan.0 = (pan.0 - (c.x - x) / width * 900.0 / W / v.zoom).clamp(-10.0, 10.0);
                            v.pan.1 = (pan.1 + (c.y - y) / height * 500.0 / H / v.zoom).clamp(-10.0, 10.0);
                        });
                    }
                },
                onpointerup: move |_| drag.set(None),
                onpointerleave: move |_| {
                    drag.set(None);
                    hovered.set(None);
                },
                onkeydown: move |e: KeyboardEvent| {
                    let mut at = (*active.peek()).min(n.saturating_sub(1));
                    match e.key() {
                        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown if e.modifiers().shift() => {
                            let (x, y) = match e.key() {
                                Key::ArrowLeft => (-0.1, 0.0),
                                Key::ArrowRight => (0.1, 0.0),
                                Key::ArrowUp => (0.0, 0.1),
                                _ => (0.0, -0.1),
                            };
                            view.with_mut(|v| {
                                v.pan.0 = (v.pan.0 + x / v.zoom).clamp(-10.0, 10.0);
                                v.pan.1 = (v.pan.1 + y / v.zoom).clamp(-10.0, 10.0);
                            });
                        }
                        Key::ArrowLeft => at = at.saturating_sub(1),
                        Key::ArrowRight => at = (at + 1).min(n.saturating_sub(1)),
                        Key::Home => at = 0,
                        Key::End => at = n.saturating_sub(1),
                        Key::Character(c) if c == " " => {
                            if let Some(ids) = key_selections.get(at) {
                                ws.select_linked(&key_panel, ids.clone());
                            }
                        }
                        Key::Enter => {
                            if let Some(ids) = key_selections.get(at) {
                                ws.select_linked(&key_panel, ids.clone());
                            }
                        }
                        Key::Character(c) if c == "+" || c == "=" => {
                            view.with_mut(|v| v.zoom = (v.zoom * 1.2).min(1000.0))
                        }
                        Key::Character(c) if c == "-" => {
                            view.with_mut(|v| v.zoom = (v.zoom / 1.2).max(0.25))
                        }
                        Key::Escape => {
                            hovered.set(None);
                            e.prevent_default();
                            e.stop_propagation();
                            return;
                        }
                        _ => return,
                    }
                    e.prevent_default();
                    e.stop_propagation();
                    active.set(at);
                    hovered.set(Some(at));
                },
                defs {
                    clipPath {
                        id: "{clip}",
                        rect { x: "100", y: "50", width: "720", height: "360" }
                    }
                }
                line { class: "mk-plot-axis", x1: "100", x2: "820", y1: "410", y2: "410" }
                line { class: "mk-plot-axis", x1: "100", x2: "100", y1: "50", y2: "410" }
                for (tick, label) in x_ticks {
                    {
                        let x = 100.0 + W * xa.unit(tick);
                        rsx! {
                            text {
                                class: "mk-plot-tick",
                                style: "font-size:{tick_font}px",
                                x: "{x}",
                                y: "432",
                                text_anchor: "middle",
                                "{label}"
                            }
                        }
                    }
                }
                for tick in ya.ticks(H * svg_scale, 50.0) {
                    {
                        let y = 410.0 - H * ya.unit(tick);
                        rsx! {
                            text {
                                class: "mk-plot-tick",
                                style: "font-size:{tick_font}px",
                                x: "90",
                                y: "{y}",
                                text_anchor: "end",
                                {ys.label(tick)}
                            }
                        }
                    }
                }
                text {
                    class: "mk-plot-label",
                    style: "font-size:{label_font}px",
                    x: "460",
                    y: "475",
                    text_anchor: "middle",
                    {x_label.chars().take(x_label_cap).collect::<String>()}
                }
                text {
                    class: "mk-plot-label",
                    style: "font-size:{label_font}px",
                    transform: "translate(20,230) rotate(-90)",
                    text_anchor: "middle",
                    {y_label.chars().take(y_label_cap).collect::<String>()}
                }
                g { clip_path: "url(#{clip})",
                    if s.mark == Mark::Line {
                        polyline { class: "mk-plot-line", points: polyline }
                    }
                    for (i, p) in points.iter().enumerate() {
                        if let Some((x, y)) = coords[i] {
                            {
                                let ids = selections[i].clone();
                                let panel = panel.clone();
                                let row = p.row;
                                let is_selected = highlighted[i];
                                let is_active = hovered() == Some(i);
                                let radius = if is_selected || is_active { 6.0 / svg_scale } else { 4.0 / svg_scale };
                                let label = tooltip(&data.0, row);
                                let class = if is_selected {
                                    "mk-plot-point mk-plot-selected"
                                } else if is_active {
                                    "mk-plot-point mk-plot-active"
                                } else {
                                    "mk-plot-point"
                                };
                                rsx! {
                                    g {
                                        class,
                                        "data-row": "{row}",
                                        "data-selected": "{is_selected}",
                                        role: "button",
                                        "aria-label": "{label}",
                                        "aria-pressed": "{is_selected}",
                                        onpointerenter: move |_| hovered.set(Some(i)),
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            if !*moved.peek() {
                                                active.set(i);
                                                ws.select_linked(&panel, ids.clone());
                                            }
                                        },
                                        title { "{label}" }
                                        if matches!(s.mark, Mark::Bar | Mark::Histogram) {
                                            {
                                                let width = (W * prepared.bar_width / (xa.hi - xa.lo)).clamp(1.0, 1000.0);
                                                let base = if ys == Scale::Linear {
                                                    (410.0 - H * ya.unit(0.0)).clamp(50.0, 410.0)
                                                } else {
                                                    410.0
                                                };
                                                rsx! {
                                                    rect {
                                                        x: "{x - width / 2.0}",
                                                        y: "{y.min(base)}",
                                                        width: "{width}",
                                                        height: "{(base - y).abs().max(1.0)}",
                                                    }
                                                }
                                            }
                                        } else {
                                            circle { cx: "{x}", cy: "{y}", r: "{radius}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if points.is_empty() {
                p { class: "mk-plot-empty", {t!(ws, L, "plot-no-values")} }
            }
            pre { class: "mk-plot-tooltip", role: "status", "{info}" }
        }
    }
}

fn mark_name(ws: Workspace, m: Mark) -> String {
    match m {
        Mark::Line => t!(ws, L, "plot-line"),
        Mark::Scatter => t!(ws, L, "plot-scatter"),
        Mark::Bar => t!(ws, L, "plot-bar"),
        Mark::Histogram => t!(ws, L, "plot-histogram"),
    }
}

fn scale_name(ws: Workspace, s: Scale) -> String {
    match s {
        Scale::Linear => t!(ws, L, "plot-linear"),
        Scale::Log => t!(ws, L, "plot-log"),
        Scale::Time => t!(ws, L, "plot-time"),
    }
}
