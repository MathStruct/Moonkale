//! The Graph panel: toolbar, two stacked canvases (wgpu + label overlay),
//! popup, and the eval channel to the renderer module.

use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use moonkale_core::{Direction, Node, NodeKind, Query, SourceFamily};
use moonkale_ext_api::Workspace;
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
}
#[derive(Serialize)]
struct OutEdge {
    a: usize,
    b: usize,
    kind: &'static str,
}
#[derive(Serialize)]
struct OutGraph<'a> {
    nodes: Vec<OutNode<'a>>,
    edges: Vec<OutEdge>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    SetGraph { graph: OutGraph<'a> },
    Fit,
    Relayout,
    Destroy,
}

#[derive(Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready { backend: String },
    Error { message: String },
    Hover { id: Option<String>, label: Option<String>, #[serde(rename = "nodeKind")] node_kind: Option<String>, key: Option<String>, x: Option<f32>, y: Option<f32> },
    Click { id: String },
    Dblclick { id: String },
    Settled,
}

fn kind_name(k: &NodeKind) -> &'static str {
    match k {
        NodeKind::File => "file",
        NodeKind::Directory => "directory",
        NodeKind::Page => "page",
        NodeKind::Symbol => "symbol",
        NodeKind::Table => "table",
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
        _ => "other",
    }
}

const SCRIPT: &str = r#"
const canvas = document.getElementById(ID + "-gl");
const overlay = document.getElementById(ID + "-ov");
if (!canvas || !overlay) { return; }
let view;
try {
    const mod = await import(JS_URL);
    await mod.default({ module_or_path: WASM_URL });
    view = await mod.create(canvas, overlay, (ev) => dioxus.send(ev));
} catch (e) {
    dioxus.send({ kind: "error", message: String(e && e.message ? e.message : e) });
    return;
}
const host = canvas.parentElement;
const ro = new ResizeObserver(() => {
    const r = host.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) view.resize(r.width, r.height, window.devicePixelRatio || 1);
});
ro.observe(host);
window.moonkale = window.moonkale || {};
(window.moonkale.graphViews = window.moonkale.graphViews || {})[ID] = view;
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setGraph") view.set_graph(JSON.stringify(msg.graph));
    else if (msg.kind === "fit") view.fit();
    else if (msg.kind === "relayout") view.relayout();
    else if (msg.kind === "destroy") { ro.disconnect(); view.destroy(); delete window.moonkale.graphViews[ID]; break; }
}
"#;

#[component]
pub fn GraphPanel(ws: Workspace) -> Element {
    let id = use_hook(|| format!("mk-graph-{}", ws.window.peek()));
    let mut eval: Signal<Option<Eval>> = use_signal(|| None);
    let mut backend = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut mode = use_signal(|| Mode::Whole);
    let mut filters = use_signal(|| Filters { files: true, symbols: true, folders: false, phantoms: true });
    let mut hover: Signal<Option<FromJs>> = use_signal(|| None);
    let mut counts = use_signal(|| (0usize, 0usize, false));
    // Nodes currently shown, by id string, so events can be resolved back to model nodes.
    let mut shown: Signal<HashMap<String, Node>> = use_signal(HashMap::new);

    // Mount the renderer once the canvases exist.
    use_effect({
        let id = id.clone();
        move || {
            if eval.read().is_some() {
                return;
            }
            let script = SCRIPT
                .replace("ID", &serde_json::to_string(&id).unwrap())
                .replace("JS_URL", &serde_json::to_string(&RENDER_JS.to_string()).unwrap())
                .replace("WASM_URL", &serde_json::to_string(&RENDER_WASM.to_string()).unwrap());
            let ev = document::eval(&script);
            let mut rx = ev;
            eval.set(Some(ev));
            spawn(async move {
                loop {
                    match rx.recv::<FromJs>().await {
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
        }
    });

    // (Re)load the graph whenever its inputs change.
    use_effect(move || {
        let index = ws.sources.read().iter().find(|s| s.descriptor.family == SourceFamily::Index).cloned();
        let m = mode();
        let f = filters();
        let active = *ws.active.read();
        let _epoch = *ws.graph_epoch.read();
        let Some(index) = index else {
            counts.set((0, 0, false));
            return;
        };
        let Some(ev) = eval.peek().clone() else { return };
        spawn(async move {
            let mut kinds = Vec::new();
            if f.files { kinds.push(NodeKind::File); }
            if f.symbols { kinds.push(NodeKind::Symbol); }
            if f.folders { kinds.push(NodeKind::Directory); }
            if f.phantoms { kinds.push(NodeKind::Page); }
            let query = match (m, active) {
                (Mode::Local, Some(node)) => Query::Neighbours { node, depth: 2, direction: Direction::Both },
                _ => Query::All { limit: 3000, kinds: Some(kinds.clone()) },
            };
            let Ok(res) = index.source.query(query).await else { return };
            let nodes: Vec<&Node> = res.nodes.iter().filter(|n| kinds.contains(&n.kind)).collect();
            let idx: HashMap<_, _> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
            let out = OutGraph {
                nodes: nodes.iter().map(|n| OutNode { id: n.id.to_string(), label: &n.label, kind: kind_name(&n.kind), key: &n.native_key }).collect(),
                edges: res
                    .edges
                    .iter()
                    .filter_map(|e| Some(OutEdge { a: *idx.get(&e.from)?, b: *idx.get(&e.to)?, kind: edge_name(&e.kind) }))
                    .collect(),
            };
            counts.set((out.nodes.len(), out.edges.len(), res.truncated));
            let _ = ev.send(ToJs::SetGraph { graph: out });
            shown.set(nodes.into_iter().map(|n| (n.id.to_string(), n.clone())).collect());
        });
    });

    let (n_nodes, n_edges, truncated) = counts();
    let has_index = ws.sources.read().iter().any(|s| s.descriptor.family == SourceFamily::Index);

    rsx! {
        document::Stylesheet { href: PANEL_CSS }
        div { class: "mk-graph",
            div { class: "mk-graph-toolbar",
                span { class: "mk-graph-modes",
                    button { class: if mode() == Mode::Whole { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| mode.set(Mode::Whole), "Whole" }
                    button { class: if mode() == Mode::Local { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| mode.set(Mode::Local), title: "Two hops around the active document", "Local" }
                }
                label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().files, onchange: move |e| filters.with_mut(|f| f.files = e.checked()) } "files" }
                label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().symbols, onchange: move |e| filters.with_mut(|f| f.symbols = e.checked()) } "symbols" }
                label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().folders, onchange: move |e| filters.with_mut(|f| f.folders = e.checked()) } "folders" }
                label { class: "mk-graph-check", input { r#type: "checkbox", checked: filters().phantoms, onchange: move |e| filters.with_mut(|f| f.phantoms = e.checked()) } "unresolved" }
                span { class: "mk-graph-spacer" }
                span { class: "mk-graph-info", "data-nodes": "{n_nodes}", "data-backend": backend().unwrap_or_default(),
                    "{n_nodes} nodes · {n_edges} edges"
                    if truncated { " · truncated" }
                    if let Some(b) = backend() { " · {b}" }
                }
                button { class: "mk-btn", onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Fit); } }, "Fit" }
                button { class: "mk-btn", onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Relayout); } }, "Relayout" }
            }
            div { class: "mk-graph-host",
                canvas { id: "{id}-gl", class: "mk-graph-canvas" }
                canvas { id: "{id}-ov", class: "mk-graph-canvas mk-graph-overlay" }
                if !has_index {
                    div { class: "mk-graph-empty", "Open a folder to see its graph." }
                }
                if let Some(err) = error() {
                    div { class: "mk-graph-empty mk-graph-error", "Renderer failed to start: {err}" }
                }
                if let Some(FromJs::Hover { id: Some(_), label, node_kind, key, x, y }) = hover() {
                    div { class: "mk-graph-popup", style: "left: {x.unwrap_or(0.0) + 14.0}px; top: {y.unwrap_or(0.0) + 14.0}px;",
                        div { class: "mk-graph-popup-title", "{label.unwrap_or_default()}" }
                        div { class: "mk-graph-popup-meta", "{node_kind.unwrap_or_default()} · {key.unwrap_or_default()}" }
                        div { class: "mk-graph-popup-hint", "double-click to open" }
                    }
                }
            }
        }
    }
}

/// Double-click: files open directly; a symbol opens the file that defines it.
async fn open_node(mut ws: Workspace, node: Node) {
    match node.kind {
        NodeKind::File => {
            if let Err(e) = ws.open_node(node).await {
                ws.set_status(e.to_string());
            }
        }
        NodeKind::Symbol => {
            let Some(index) = ws.index() else { return };
            if let Ok(res) = index.source.query(Query::Neighbours { node: node.id, depth: 1, direction: Direction::In }).await {
                if let Some(file) = res.nodes.into_iter().find(|n| n.kind == NodeKind::File) {
                    if let Err(e) = ws.open_node(file).await {
                        ws.set_status(e.to_string());
                    }
                }
            }
        }
        _ => ws.set_status(format!("{} has nothing to open", node.label)),
    }
}
