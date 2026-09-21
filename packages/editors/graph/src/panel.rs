//! The Graph panel: toolbar, two stacked canvases (wgpu + label overlay),
//! popup, and the eval channel to the renderer module.

use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use moonkale_core::{Direction, Node, NodeKind, Query, SourceFamily, SourceId};
use moonkale_ext_api::{GraphRequest, Workspace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const PANEL_CSS: Asset = asset!("/assets/panel.css");
const RENDER_JS: Asset = asset!("/assets/graph_render.js");
const RENDER_WASM: Asset = asset!("/assets/graph_render_bg.wasm");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Whole,
    Local,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Filters {
    files: bool,
    symbols: bool,
    folders: bool,
    phantoms: bool,
}

#[derive(Serialize)]
struct OutNode<'a> {
    id: String,
    label: &'a str,
    kind: &'static str,
    key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'static str>,
}
#[derive(Serialize)]
struct OutEdge {
    a: usize,
    b: usize,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'static str>,
}

/// What to draw for a database source.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DbMode {
    /// Tables and properties; rel tables as edges.
    Schema,
    /// The stored nodes and relations (capped).
    Data,
    /// The last *Show in Graph* query from a table editor.
    Query,
}

/// Twelve distinguishable hues for node labels / relation names; a label
/// always maps to the same colour (hash), so the legend and the drawing agree.
const PALETTE: [&str; 12] = [
    "#f2a03d", "#4fb3e8", "#8ccf6a", "#e86b8a", "#b48cf0", "#f0d55a", "#4fd6c2", "#f07c4f",
    "#9ab7ff", "#d69bd6", "#a8d86b", "#ff9fb3",
];

fn label_color(label: &str) -> &'static str {
    let h = label
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32));
    PALETTE[(h % PALETTE.len() as u32) as usize]
}

/// Nodes drawn at most. Barnes–Hut layout and label LOD (Milestone 6) keep
/// 100k usable; the index itself caps files at 5 000, so this only matters
/// for databases and synthetic graphs.
const GRAPH_LIMIT: usize = 100_000;

/// Sources the picker offers besides the index: databases and traces.
fn is_pickable(f: &SourceFamily) -> bool {
    matches!(f, SourceFamily::Graph | SourceFamily::Sql) || is_trace(f)
}

/// In-memory graph sources that arrive from elsewhere: traces (Milestone 4)
/// and git history (Milestone 7). Pickable, auto-picked, file nodes open
/// by path.
fn is_trace(f: &SourceFamily) -> bool {
    matches!(f, SourceFamily::Custom(c) if c == "trace" || c == "git")
}

/// `v:<Label>:<table>:<offset>` → `Label` (see `sources-graph::ladybug`).
fn vertex_group(node: &Node) -> Option<&str> {
    node.native_key.strip_prefix("v:")?.split(':').next()
}
#[derive(Serialize)]
struct OutGraph<'a> {
    nodes: Vec<OutNode<'a>>,
    edges: Vec<OutEdge>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    SetGraph {
        graph: OutGraph<'a>,
    },
    Fit,
    Relayout,
    /// `"2d"` | `"3d"` (Milestone 8).
    SetMode {
        mode: &'a str,
    },
    Destroy,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Loaded,
    Ready {
        backend: String,
    },
    Error {
        message: String,
    },
    Hover {
        id: Option<String>,
        label: Option<String>,
        #[serde(rename = "nodeKind")]
        node_kind: Option<String>,
        key: Option<String>,
        x: Option<f32>,
        y: Option<f32>,
    },
    Click {
        id: String,
    },
    Dblclick {
        id: String,
    },
    Settled,
}

fn kind_name(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::File => "file",
        NodeKind::Directory => "directory",
        NodeKind::Page => "page",
        NodeKind::Symbol => "symbol",
        NodeKind::Table => "table",
        NodeKind::Database => "database",
        NodeKind::Column => "column",
        NodeKind::Vertex => "vertex",
        _ => "other",
    }
}

fn edge_name(k: &moonkale_core::EdgeKind) -> &'static str {
    use moonkale_core::EdgeKind as E;
    match k {
        E::Links => "links",
        E::Defines => "defines",
        E::Contains => "contains",
        E::References => "references",
        E::Custom(_) => "custom",
        _ => "other",
    }
}

const SCRIPT: &str = r#"
// Handshake: messages sent before the host awaits are lost, so wait for
// the host's first message before sending anything.
await dioxus.recv();
const canvas = document.getElementById(ID + "-gl");
const overlay = document.getElementById(ID + "-ov");
if (!canvas || !overlay) { return; }
// Load the module first (proves the assets are served), then check for a
// GPU API: without one, wgpu's create() would hang instead of failing.
const failed = async (message) => {
    dioxus.send({ kind: "error", message });
    // A finished eval drops unread messages: wait for destroy instead of returning.
    for (;;) { const m = await dioxus.recv(); if (m.kind === "destroy") return; }
};
let mod;
try {
    // Absolute URLs: an eval has no script origin, so a relative import()
    // resolves against about:blank in the Android WebView (P-088).
    const abs = (u) => new URL(u, document.baseURI || location.href).href;
    mod = await import(abs(JS_URL));
    await mod.default({ module_or_path: abs(WASM_URL) });
    dioxus.send({ kind: "loaded" });
} catch (e) {
    return failed("could not load the renderer module: " + String(e && e.message ? e.message : e));
}
const probe = document.createElement("canvas");
const probeGl = probe.getContext("webgl2");
if (!probeGl && !navigator.gpu) {
    return failed("this browser/webview offers neither WebGPU nor WebGL2");
}
// Don't keep a second GL context alive just for the probe.
try { probeGl && probeGl.getExtension("WEBGL_lose_context")?.loseContext(); } catch (_) {}
let view;
try {
    const timeout = new Promise((_, rej) => setTimeout(() => rej(new Error("renderer did not start within 15s")), 15000));
    // Android WebViews advertise WebGPU but hang on device creation: ask for WebGL2 there (P-089).
    const prefer = /\bAndroid\b/.test(navigator.userAgent) && /\bwv\b/.test(navigator.userAgent) ? "gl" : null;
    view = await Promise.race([mod.create(canvas, overlay, (ev) => dioxus.send(ev), prefer), timeout]);
} catch (e) {
    return failed(String(e && e.message ? e.message : e));
}
const host = canvas.parentElement;
const ro = new ResizeObserver(() => {
    const r = host.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) view.resize(r.width, r.height, window.devicePixelRatio || 1);
});
ro.observe(host);
window.moonkale = window.moonkale || {};
(window.moonkale.graphViews = window.moonkale.graphViews || {})[ID] = view;
// Tear the GL context down before the page goes away (window close, reload):
// the NVIDIA EGL driver crashes WebKit's web process when it exits with a
// live WebGL context (P-061). Only helps on graceful unloads.
const unload = () => {
    try { view.destroy(); } catch (_) {}
    try { const gl = canvas.getContext("webgl2") || canvas.getContext("webgl"); gl && gl.getExtension("WEBGL_lose_context")?.loseContext(); } catch (_) {}
};
window.addEventListener("pagehide", unload, { once: true });
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setGraph") view.set_graph(JSON.stringify(msg.graph));
    else if (msg.kind === "fit") view.fit();
    else if (msg.kind === "relayout") view.relayout();
    else if (msg.kind === "setMode") view.set_mode(msg.mode);
    else if (msg.kind === "destroy") { ro.disconnect(); view.destroy(); delete window.moonkale.graphViews[ID]; break; }
}
"#;

#[component]
pub fn GraphPanel(ws: Workspace) -> Element {
    // Deterministic: the page is server-rendered first and hydration keeps the
    // server's element ids, so anything random here would never match (P-034).
    // One graph panel per window/tab, so a constant is enough.
    let id = use_hook(|| "mk-graph-panel".to_string());
    let mut eval: Signal<Option<Eval>> = use_signal(|| None);
    let mut backend = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut mode = use_signal(|| Mode::Whole);
    let mut filters = use_signal(|| Filters {
        files: true,
        symbols: true,
        folders: false,
        phantoms: true,
    });
    let mut hover: Signal<Option<FromJs>> = use_signal(|| None);
    // What to draw: the index (default), a database's schema, or the last
    // "Show in Graph" query. `None` = index.
    let mut picked: Signal<Option<SourceId>> = use_signal(|| None);
    let mut db_mode = use_signal(|| DbMode::Data);
    // Labels drawn in the current database view, for the legend.
    let mut legend: Signal<Vec<(String, &'static str)>> = use_signal(Vec::new);
    let mut seen_request: Signal<Option<GraphRequest>> = use_signal(|| None);
    let mut counts = use_signal(|| (0usize, 0usize, false));
    // Nodes currently shown, by id string, so events can be resolved back to model nodes.
    let mut shown: Signal<HashMap<String, Node>> = use_signal(HashMap::new);

    // Mount the renderer from the host element's `onmounted` event (an event
    // handler, not an effect: nothing here re-runs or drops the eval later).
    let mount = {
        let id = id.clone();
        move |_| {
            if eval.peek().is_some() {
                return;
            }
            let script = SCRIPT
                .replace("ID", &serde_json::to_string(&id).unwrap())
                .replace(
                    "JS_URL",
                    &serde_json::to_string(&RENDER_JS.to_string()).unwrap(),
                )
                .replace(
                    "WASM_URL",
                    &serde_json::to_string(&RENDER_WASM.to_string()).unwrap(),
                );
            let ev = document::eval(&script);
            let mut rx = ev;
            spawn(async move {
                loop {
                    match rx.recv::<FromJs>().await {
                        Ok(FromJs::Loaded) => loaded.set(true),
                        Ok(FromJs::Ready { backend: b }) => backend.set(Some(b)),
                        Ok(FromJs::Error { message }) => error.set(Some(message)),
                        Ok(h @ FromJs::Hover { .. }) => hover.set(Some(h)),
                        Ok(FromJs::Dblclick { id }) => {
                            let node = shown.peek().get(&id).cloned();
                            if let Some(node) = node {
                                spawn(open_node(ws, node));
                            }
                        }
                        Ok(FromJs::Click { .. }) | Ok(FromJs::Settled) => {}
                        Err(dioxus::document::EvalError::Serialization(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            // The JS side waits for this before it talks (see SCRIPT).
            let _ = ev.send(serde_json::json!({ "kind": "init" }));
            eval.set(Some(ev));
        }
    };

    // Tear the renderer down when the panel unmounts (dock/undock remounts it).
    use_drop(move || {
        if let Some(ev) = eval.peek().as_ref() {
            let _ = ev.send(ToJs::Destroy);
        }
    });

    // A graph database that was just opened is what the user wants to see:
    // switch the picker to it (the picker still offers the index).
    let mut seen_graph_sources: Signal<Vec<SourceId>> = use_signal(Vec::new);
    let mut load_gen: Signal<u64> = use_signal(|| 0);
    use_effect(move || {
        let graph_ids: Vec<SourceId> = ws
            .sources
            .read()
            .iter()
            .filter(|s| {
                s.descriptor.family == SourceFamily::Graph || is_trace(&s.descriptor.family)
            })
            .map(|s| s.descriptor.id.clone())
            .collect();
        let new = graph_ids
            .iter()
            .find(|id| !seen_graph_sources.peek().contains(id))
            .cloned();
        if let Some(id) = new {
            picked.set(Some(id));
            db_mode.set(DbMode::Data);
        }
        if *seen_graph_sources.peek() != graph_ids {
            seen_graph_sources.set(graph_ids);
        }
    });

    // A new "Show in Graph" request switches the picker to that query.
    use_effect(move || {
        let req = ws.graph_request.read().clone();
        if req.is_some() && req != *seen_request.peek() {
            seen_request.set(req.clone());
            picked.set(req.map(|r| r.source));
            db_mode.set(DbMode::Query);
        }
    });

    // (Re)load the graph whenever its inputs change.
    use_effect(move || {
        // Every open folder's index goes into one graph (spec 020: two
        // repositories, two vaults to merge); the picker narrows to one.
        let indices: Vec<_> = ws
            .sources
            .read()
            .iter()
            .filter(|s| s.descriptor.family == SourceFamily::Index)
            .cloned()
            .collect();
        let m = mode();
        let f = filters();
        let active = *ws.active.read();
        let _epoch = *ws.graph_epoch.read();
        let picked_id = picked();
        let dbm = db_mode();
        let request = if dbm == DbMode::Query {
            ws.graph_request.read().clone()
        } else {
            None
        };
        let Some(ev) = *eval.peek() else {
            return;
        };
        // Loads are async and may finish out of order (a remote index reply
        // after a local trace); only the newest load may publish.
        let gen = *load_gen.peek() + 1;
        load_gen.set(gen);
        let picked_index = picked_id
            .as_ref()
            .and_then(|pid| indices.iter().find(|s| &s.descriptor.id == pid).cloned());
        // A database source: draw its schema (or the requested query's result).
        if let (Some(pid), None) = (picked_id.clone(), &picked_index) {
            let Some(handle) = ws
                .sources
                .read()
                .iter()
                .find(|s| s.descriptor.id == pid)
                .cloned()
            else {
                counts.set((0, 0, false));
                return;
            };
            spawn(async move {
                let _ = (m, active);
                let query = match (&request, dbm) {
                    (Some(r), DbMode::Query) if r.source == pid => Query::Text {
                        dialect: r.dialect.clone(),
                        text: r.text.clone(),
                    },
                    (_, DbMode::Data) => Query::All {
                        limit: GRAPH_LIMIT,
                        kinds: Some(vec![NodeKind::Vertex]),
                    },
                    _ => Query::All {
                        limit: GRAPH_LIMIT,
                        kinds: None,
                    },
                };
                let res = match handle.source.query(query).await {
                    Ok(r) => r,
                    Err(e) => {
                        error.set(Some(e.to_string()));
                        return;
                    }
                };
                if *load_gen.peek() != gen {
                    return;
                }
                let idx: HashMap<_, _> = res
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.id, i))
                    .collect();
                // Vertices are coloured by their label (node table), relations
                // by their name; the schema keeps the kind colours.
                let mut groups: Vec<(String, &'static str)> = Vec::new();
                let out = OutGraph {
                    nodes: res
                        .nodes
                        .iter()
                        .map(|n| {
                            let color = vertex_group(n).map(|g| {
                                let c = label_color(g);
                                if !groups.iter().any(|(l, _)| l == g) {
                                    groups.push((g.to_string(), c));
                                }
                                c
                            });
                            OutNode {
                                id: n.id.to_string(),
                                label: &n.label,
                                kind: kind_name(&n.kind),
                                key: &n.native_key,
                                color,
                            }
                        })
                        .collect(),
                    edges: res
                        .edges
                        .iter()
                        .filter_map(|e| {
                            let color = match &e.kind {
                                moonkale_core::EdgeKind::Custom(name) => Some(label_color(name)),
                                _ => None,
                            };
                            Some(OutEdge {
                                a: *idx.get(&e.from)?,
                                b: *idx.get(&e.to)?,
                                kind: edge_name(&e.kind),
                                color,
                            })
                        })
                        .collect(),
                };
                groups.sort();
                legend.set(groups);
                counts.set((out.nodes.len(), out.edges.len(), res.truncated));
                let _ = ev.send(ToJs::SetGraph { graph: out });
                shown.set(
                    res.nodes
                        .iter()
                        .map(|n| (n.id.to_string(), n.clone()))
                        .collect(),
                );
            });
            return;
        }
        let indices: Vec<_> = match picked_index {
            Some(one) => vec![one],
            None => indices,
        };
        if indices.is_empty() {
            counts.set((0, 0, false));
            return;
        }
        spawn(async move {
            let mut kinds = Vec::new();
            if f.files {
                kinds.push(NodeKind::File);
            }
            if f.symbols {
                kinds.push(NodeKind::Symbol);
            }
            if f.folders {
                kinds.push(NodeKind::Directory);
            }
            if f.phantoms {
                kinds.push(NodeKind::Page);
            }
            let query = match (m, active) {
                (Mode::Local, Some(node)) => Query::Neighbours {
                    node,
                    depth: 2,
                    direction: Direction::Both,
                },
                _ => Query::All {
                    limit: GRAPH_LIMIT,
                    kinds: Some(kinds.clone()),
                },
            };
            // One query per index, merged: ids are derived per index so
            // they never collide; with several folders each gets a colour.
            let mut res = moonkale_core::QueryResult::default();
            let mut owner: Vec<usize> = Vec::new();
            let mut groups: Vec<(String, &'static str)> = Vec::new();
            for (i, index) in indices.iter().enumerate() {
                let Ok(r) = index.source.query(query.clone()).await else {
                    continue;
                };
                owner.extend(std::iter::repeat_n(i, r.nodes.len()));
                res.nodes.extend(r.nodes);
                res.edges.extend(r.edges);
                res.truncated |= r.truncated;
                if indices.len() > 1 {
                    let name = index
                        .descriptor
                        .id
                        .as_str()
                        .rsplit('/')
                        .next()
                        .unwrap_or(index.descriptor.id.as_str())
                        .to_string();
                    groups.push((name.clone(), label_color(&name)));
                }
            }
            if *load_gen.peek() != gen {
                return;
            }
            let nodes: Vec<(&Node, Option<&'static str>)> = res
                .nodes
                .iter()
                .zip(owner.iter())
                .filter(|(n, _)| kinds.contains(&n.kind))
                .map(|(n, o)| (n, groups.get(*o).map(|g| g.1)))
                .collect();
            let idx: HashMap<_, _> = nodes
                .iter()
                .enumerate()
                .map(|(i, (n, _))| (n.id, i))
                .collect();
            let out = OutGraph {
                nodes: nodes
                    .iter()
                    .map(|(n, color)| OutNode {
                        id: n.id.to_string(),
                        label: &n.label,
                        kind: kind_name(&n.kind),
                        key: &n.native_key,
                        color: *color,
                    })
                    .collect(),
                edges: res
                    .edges
                    .iter()
                    .filter_map(|e| {
                        Some(OutEdge {
                            a: *idx.get(&e.from)?,
                            b: *idx.get(&e.to)?,
                            kind: edge_name(&e.kind),
                            color: None,
                        })
                    })
                    .collect(),
            };
            counts.set((out.nodes.len(), out.edges.len(), res.truncated));
            legend.set(groups);
            let _ = ev.send(ToJs::SetGraph { graph: out });
            shown.set(
                nodes
                    .into_iter()
                    .map(|(n, _)| (n.id.to_string(), n.clone()))
                    .collect(),
            );
        });
    });

    let (n_nodes, n_edges, truncated) = counts();
    let has_index = ws
        .sources
        .read()
        .iter()
        .any(|s| s.descriptor.family == SourceFamily::Index);
    // Sources worth drawing besides "every folder": databases (schema
    // graphs), and each folder's own index when more than one is open.
    let index_count = ws
        .sources
        .read()
        .iter()
        .filter(|s| s.descriptor.family == SourceFamily::Index)
        .count();
    let databases: Vec<(SourceId, String)> = ws
        .sources
        .read()
        .iter()
        .filter(|s| {
            is_pickable(&s.descriptor.family)
                || (index_count > 1 && s.descriptor.family == SourceFamily::Index)
        })
        .map(|s| {
            let name = if s.descriptor.family == SourceFamily::Index {
                format!(
                    "folder: {}",
                    s.descriptor.id.as_str().rsplit('/').next().unwrap_or("?")
                )
            } else {
                s.descriptor.display_name.clone()
            };
            (s.descriptor.id.clone(), name)
        })
        .collect();
    let picked_is_index = picked()
        .and_then(|p| {
            ws.sources
                .read()
                .iter()
                .find(|s| s.descriptor.id == p)
                .map(|s| s.descriptor.family == SourceFamily::Index)
        })
        .unwrap_or(false);
    let picked_is_trace = picked()
        .and_then(|p| {
            ws.sources
                .read()
                .iter()
                .find(|s| s.descriptor.id == p)
                .map(|s| is_trace(&s.descriptor.family))
        })
        .unwrap_or(false);
    let mut paste_open = use_signal(|| false);
    let mut three_d = use_signal(|| false);
    let mut paste_text = use_signal(String::new);
    let has_request = ws.graph_request.read().is_some();
    let picked_str = picked().map(|p| p.to_string()).unwrap_or_default();
    let is_index = picked().is_none() || picked_is_index;
    let has_any = has_index || !databases.is_empty();

    rsx! {
        moonkale_ext_api::Stylesheet { href: PANEL_CSS }
        div { class: "mk-graph",
            div { class: "mk-graph-toolbar",
                if !databases.is_empty() {
                    select { class: "mk-graph-source", title: "Which source to draw",
                        value: "{picked_str}",
                        onchange: move |e| {
                            let v = e.value();
                            picked.set(if v.is_empty() { None } else { Some(SourceId::new(v)) });
                            if db_mode() == DbMode::Query { db_mode.set(DbMode::Data); }
                        },
                        option { value: "", selected: picked().is_none(), if index_count > 1 { "all folders" } else { "index" } }
                        for (sid, name) in databases.iter() {
                            option { key: "{sid}", value: "{sid}", selected: picked().as_ref() == Some(sid), "{name}" }
                        }
                    }
                }
                if picked_is_trace {
                    span { class: "mk-graph-modes", span { class: "mk-muted", "files → frames → call chain" } }
                } else if is_index {
                    span { class: "mk-graph-modes",
                        button { class: if mode() == Mode::Whole { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| mode.set(Mode::Whole), "Whole" }
                        button { class: if mode() == Mode::Local { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| mode.set(Mode::Local), title: "Two hops around the active document", "Local" }
                    }
                } else {
                    span { class: "mk-graph-modes",
                        button { class: if db_mode() == DbMode::Data { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| db_mode.set(DbMode::Data), title: "The stored nodes and relations (up to 3000)", "Data" }
                        button { class: if db_mode() == DbMode::Schema { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| db_mode.set(DbMode::Schema), title: "Tables and properties; relation tables as edges", "Schema" }
                        if has_request {
                            button { class: if db_mode() == DbMode::Query { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| db_mode.set(DbMode::Query), title: "The last query sent from a table editor", "Query" }
                        }
                    }
                    if db_mode() != DbMode::Schema {
                        span { class: "mk-graph-legend",
                            for (name, color) in legend() {
                                span { key: "{name}", class: "mk-graph-legend-item", span { class: "mk-graph-swatch", style: "background: {color}" } "{name}" }
                            }
                        }
                    }
                }
                if is_index && !legend().is_empty() {
                    span { class: "mk-graph-legend",
                        for (name, color) in legend() {
                            span { key: "{name}", class: "mk-graph-legend-item", span { class: "mk-graph-swatch", style: "background: {color}" } "{name}" }
                        }
                    }
                }
                if is_index {
                    label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().files, onchange: move |e| filters.with_mut(|f| f.files = e.checked()) } "files" }
                    label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().symbols, onchange: move |e| filters.with_mut(|f| f.symbols = e.checked()) } "symbols" }
                    label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().folders, onchange: move |e| filters.with_mut(|f| f.folders = e.checked()) } "folders" }
                    label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().phantoms, onchange: move |e| filters.with_mut(|f| f.phantoms = e.checked()) } "unresolved" }
                }
                span { class: "mk-graph-spacer" }
                span { class: "mk-graph-info", "data-nodes": "{n_nodes}", "data-mode": if three_d() { "3d" } else { "2d" }, "data-backend": backend().unwrap_or_default(), "data-module": if loaded() { "loaded" } else { "" },
                    "{n_nodes} nodes · {n_edges} edges"
                    if truncated { " · truncated" }
                    if let Some(b) = backend() { " · {b}" }
                }
                button { class: if paste_open() { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| paste_open.toggle(), title: "Paste a stack trace or compiler output and draw it", "Trace…" }
                button { class: "mk-btn", onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Fit); } }, "Fit" }
                button { class: "mk-btn", onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Relayout); } }, "Relayout" }
                button { class: if three_d() { "mk-btn mk-btn-on" } else { "mk-btn" }, title: "3D: one plane per node kind; drag to pan, right-drag or Shift-drag to orbit, wheel to dolly",
                    onclick: move |_| {
                        let next = !three_d();
                        three_d.set(next);
                        if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::SetMode { mode: if next { "3d" } else { "2d" } }); }
                    },
                    "3D"
                }
            }
            if paste_open() {
                div { class: "mk-graph-paste",
                    textarea { class: "mk-graph-paste-text", rows: 6, placeholder: "Paste a Rust panic/backtrace, cargo errors, a Python traceback or a JS stack…",
                        value: "{paste_text}", oninput: move |e| paste_text.set(e.value()) }
                    div { class: "mk-graph-paste-actions",
                        button { class: "mk-btn mk-btn-on", onclick: move |_| {
                            let mut ws = ws;
                            let traces = moonkale_trace::parse(&paste_text.peek());
                            if traces.is_empty() {
                                ws.set_status("No trace found in the pasted text");
                            } else {
                                for t in &traces {
                                    let unique = ws.next_unique();
                                    ws.add_source(std::sync::Arc::new(moonkale_trace::TraceSource::new(t, unique)));
                                }
                                paste_open.set(false);
                            }
                        }, "Draw" }
                        button { class: "mk-btn", onclick: move |_| paste_open.set(false), "Cancel" }
                    }
                }
            }
            div { class: "mk-graph-host", onmounted: mount,
                canvas { id: "{id}-gl", class: "mk-graph-canvas" }
                canvas { id: "{id}-ov", class: "mk-graph-canvas mk-graph-overlay" }
                if !has_any {
                    div { class: "mk-graph-empty", "Open a folder or a database to see its graph." }
                }
                if let (true, Some(err)) = (has_any, error()) {
                    div { class: "mk-graph-empty mk-graph-error", "Renderer failed to start: {err}" }
                }
                if let Some(FromJs::Hover { id: Some(_), label, node_kind, key, x, y }) = hover() {
                    {
                        let label = label.unwrap_or_default();
                        let meta = format!("{} · {}", node_kind.unwrap_or_default(), key.unwrap_or_default());
                        let (px, py) = (x.unwrap_or(0.0) + 14.0, y.unwrap_or(0.0) + 14.0);
                        rsx! {
                            div { class: "mk-graph-popup", style: "left: {px}px; top: {py}px;",
                                div { class: "mk-graph-popup-title", "{label}" }
                                div { class: "mk-graph-popup-meta", "{meta}" }
                                div { class: "mk-graph-popup-hint", "double-click to open" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Double-click: files open directly; a symbol opens the file that defines it.
async fn open_node(mut ws: Workspace, node: Node) {
    // Trace nodes point into the folder: `path` or `path:line[:col]`.
    let from_trace = ws
        .sources
        .peek()
        .iter()
        .any(|s| s.descriptor.id == node.source && is_trace(&s.descriptor.family));
    if from_trace {
        if let Some(hash) = node.native_key.strip_prefix("commit:") {
            ws.set_status(format!("Commit {hash}: {}", node.label));
            return;
        }
        let (path, line, col) = split_location(&node.native_key);
        match ws.open_relative_path(&path).await {
            Ok(n) => {
                let _ = ws
                    .reveal(n, line.saturating_sub(1), col.saturating_sub(1))
                    .await;
            }
            Err(_) => ws.set_status(format!("{path} is not in an open folder")),
        }
        return;
    }
    match node.kind {
        NodeKind::File => {
            if let Err(e) = ws.open_node(node).await {
                ws.set_status(e.to_string());
            }
        }
        NodeKind::Symbol => {
            let Some(index) = ws.index() else { return };
            if let Ok(res) = index
                .source
                .query(Query::Neighbours {
                    node: node.id,
                    depth: 1,
                    direction: Direction::In,
                })
                .await
            {
                if let Some(file) = res.nodes.into_iter().find(|n| n.kind == NodeKind::File) {
                    if let Err(e) = ws.open_node(file).await {
                        ws.set_status(e.to_string());
                    }
                }
            }
        }
        NodeKind::Table => {
            if let Err(e) = ws.open_node(node).await {
                ws.set_status(e.to_string());
            }
        }
        _ => ws.set_status(format!("{} has nothing to open", node.label)),
    }
}

/// `path[:line[:col]]` → `(path, line, col)` (0 when absent).
fn split_location(key: &str) -> (String, u32, u32) {
    let mut parts = key.rsplitn(3, ':');
    let a = parts.next().unwrap_or("");
    let b = parts.next();
    let c = parts.next();
    match (a.parse::<u32>(), b.and_then(|x| x.parse::<u32>().ok()), c) {
        (Ok(col), Some(line), Some(path)) => (path.to_string(), line, col),
        (Ok(line), _, _) if b.is_some() => (key[..key.len() - a.len() - 1].to_string(), line, 0),
        _ => (key.to_string(), 0, 0),
    }
}
