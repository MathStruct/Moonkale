//! The flow editor panel: a dioxus-flow canvas over one `*.flow.json`
//! document, a palette of the enabled libraries' blocks, typed wiring,
//! per-block parameters, validation and codegen.
//!
//! State: the canvas' `nodes`/`edges` signals are the working copy; every
//! change is serialised back into the `Document` (dirty → Save, like any
//! editor). The document is re-read into the canvas only when its text is
//! replaced from outside (reload).

use dioxus::prelude::*;
use dioxus_flow::prelude::*;
use dioxus_flow::Id;
use moonkale_core::NodeId as CoreNodeId;
use moonkale_ext_api::flow::{find_kind, validate, Block, Flow, FlowLibrary, ParamKind, Wire};
use moonkale_ext_api::Workspace;
use std::collections::BTreeMap;

const CSS: Asset = asset!("/assets/flow.css");

/// Per-node payload: which block kind, its parameters, and any issues.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockData {
    pub kind: String,
    pub params: BTreeMap<String, String>,
    pub issues: Vec<String>,
}

fn to_canvas(flow: &Flow, libs: &[FlowLibrary]) -> (Vec<Node<BlockData>>, Vec<Edge>) {
    let issues = validate(flow, libs);
    let nodes = flow
        .blocks
        .iter()
        .map(|b| {
            let name = find_kind(libs, &b.kind)
                .map(|(_, k)| k.name.clone())
                .unwrap_or_else(|| b.kind.clone());
            let mut n = Node::with_data(
                b.id.clone(),
                name,
                (b.x, b.y),
                BlockData {
                    kind: b.kind.clone(),
                    params: b.params.clone(),
                    issues: issues
                        .iter()
                        .filter(|i| i.about == b.id)
                        .map(|i| i.message.clone())
                        .collect(),
                },
            );
            n.source_side = Side::Right;
            n.target_side = Side::Left;
            n
        })
        .collect();
    let edges = flow
        .wires
        .iter()
        .map(|w| {
            let mut e = Edge::new(w.from_block.clone(), w.to_block.clone());
            e.id = w.id.clone();
            e.source_handle = Some(w.from_port.clone());
            e.target_handle = Some(w.to_port.clone());
            if issues.iter().any(|i| i.about == w.id) {
                e.class = Some("mk-flow-wire-bad".into());
            }
            e
        })
        .collect();
    (nodes, edges)
}

fn to_flow(nodes: &[Node<BlockData>], edges: &[Edge]) -> Flow {
    let mut f = Flow::new();
    f.blocks = nodes
        .iter()
        .map(|n| Block {
            id: n.id.to_string(),
            kind: n.data.kind.clone(),
            x: n.position.x,
            y: n.position.y,
            params: n.data.params.clone(),
        })
        .collect();
    f.wires = edges
        .iter()
        .map(|e| Wire {
            id: e.id.to_string(),
            from_block: e.source.to_string(),
            from_port: e
                .source_handle
                .as_ref()
                .map(|h| h.to_string())
                .unwrap_or_default(),
            to_block: e.target.to_string(),
            to_port: e
                .target_handle
                .as_ref()
                .map(|h| h.to_string())
                .unwrap_or_default(),
        })
        .collect();
    f
}

#[component]
pub fn FlowPanel(ws: Workspace, node: CoreNodeId) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let libs = ws.flow_libraries.read().clone();
    let initial = Flow::parse(&doc.peek().text).unwrap_or_default();
    let (n0, e0) = to_canvas(&initial, &libs);
    let mut nodes: Signal<Vec<Node<BlockData>>> = use_signal(|| n0);
    let mut edges: Signal<Vec<Edge>> = use_signal(|| e0);
    let mut last_text = use_signal(|| doc.peek().text.clone());
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut selected_lib = use_signal(|| 0usize);
    let handle: FlowHandle<BlockData> = use_flow_handle();

    // Canvas → document (dirty), and re-validate.
    let libs_for_sync = libs.clone();
    let sync = move || {
        let flow = to_flow(&nodes.peek(), &edges.peek());
        let issues = validate(&flow, &libs_for_sync);
        nodes.with_mut(|ns| {
            for n in ns.iter_mut() {
                n.data.issues = issues
                    .iter()
                    .filter(|i| i.about == n.id)
                    .map(|i| i.message.clone())
                    .collect();
            }
        });
        edges.with_mut(|es| {
            for e in es.iter_mut() {
                e.class = issues
                    .iter()
                    .any(|i| i.about == e.id)
                    .then(|| "mk-flow-wire-bad".to_string());
            }
        });
        let text = flow.to_json();
        last_text.set(text.clone());
        doc.with_mut(|d| d.text = text);
    };

    // Document replaced from outside (reload): rebuild the canvas.
    {
        let libs = libs.clone();
        use_effect(move || {
            let text = doc.read().text.clone();
            if text == *last_text.peek() {
                return;
            }
            last_text.set(text.clone());
            let flow = Flow::parse(&text).unwrap_or_default();
            let (n, e) = to_canvas(&flow, &libs);
            nodes.set(n);
            edges.set(e);
        });
    }

    let libs_valid = libs.clone();
    let is_valid = move |c: Connection| -> bool {
        let ns = nodes.peek();
        let kind_of = |id: &Id| ns.iter().find(|n| &n.id == id).map(|n| n.data.kind.clone());
        let (Some(fk), Some(tk)) = (kind_of(&c.source), kind_of(&c.target)) else {
            return false;
        };
        let (Some((_, fkind)), Some((_, tkind))) =
            (find_kind(&libs_valid, &fk), find_kind(&libs_valid, &tk))
        else {
            return false;
        };
        let out = fkind
            .outputs
            .iter()
            .find(|p| Some(p.name.as_str()) == c.source_handle.as_deref());
        let inp = tkind
            .inputs
            .iter()
            .find(|p| Some(p.name.as_str()) == c.target_handle.as_deref());
        match (out, inp) {
            (Some(o), Some(i)) => o.ty.unify(&i.ty).is_some(),
            _ => false,
        }
    };

    let on_connect = {
        let mut sync = sync.clone();
        move |c: Connection| {
            // One wire per input port: a new one replaces the old.
            edges.with_mut(|es| {
                es.retain(|e| !(e.target == c.target && e.target_handle == c.target_handle));
                let mut e = c.into_edge();
                e.id = format!("w-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
                es.push(e);
            });
            sync();
        }
    };
    let on_drag_stop = {
        let mut sync = sync.clone();
        move |_ids: Vec<Id>| sync()
    };
    let on_delete = {
        let mut sync = sync.clone();
        move |req: DeleteRequest| {
            nodes.with_mut(|ns| ns.retain(|n| !req.nodes.contains(&n.id)));
            edges.with_mut(|es| {
                es.retain(|e| {
                    !req.edges.contains(&e.id)
                        && !req.nodes.contains(&e.source)
                        && !req.nodes.contains(&e.target)
                })
            });
            sync();
        }
    };

    let add_block = {
        let libs = libs.clone();
        let mut sync = sync.clone();
        move |kind: String| {
            let Some((lib, k)) = find_kind(&libs, &kind) else {
                return;
            };
            // New blocks go into a 3-column grid so they never pile up.
            let n = nodes.peek().len();
            let (col, row) = ((n % 3) as f64, (n / 3) as f64);
            let params = k
                .params
                .iter()
                .map(|p| (p.name.clone(), p.default.clone()))
                .collect();
            let mut node = Node::with_data(
                format!(
                    "{}-{}",
                    k.id,
                    &uuid::Uuid::new_v4().simple().to_string()[..6]
                ),
                k.name.clone(),
                (30.0 + col * 165.0, 30.0 + row * 170.0),
                BlockData {
                    kind: format!("{}/{}", lib.id, k.id),
                    params,
                    issues: Vec::new(),
                },
            );
            node.source_side = Side::Right;
            node.target_side = Side::Left;
            nodes.with_mut(|ns| ns.push(node));
            sync();
        }
    };

    let set_param = {
        let mut sync = sync.clone();
        move |(id, name, value): (Id, String, String)| {
            nodes.with_mut(|ns| {
                if let Some(n) = ns.iter_mut().find(|n| n.id == id) {
                    n.data.params.insert(name, value);
                }
            });
            sync();
        }
    };

    let libs_view = libs.clone();
    let node_view = Callback::new(move |ctx: NodeViewCtx<BlockData>| {
        let n = ctx.node.clone();
        let kind = find_kind(&libs_view, &n.data.kind).map(|(_, k)| k.clone());
        let bad = !n.data.issues.is_empty();
        let id = n.id.clone();
        rsx! {
            div { class: if bad { "mk-flow-block mk-flow-block-bad" } else { "mk-flow-block" }, title: "{n.data.issues.join(\"\\n\")}",
                div { class: "mk-flow-block-title", "{n.label}" }
                if let Some(k) = kind {
                    for (i, p) in k.inputs.iter().enumerate() {
                        Handle { kind: HandleKind::Target, position: Side::Left, id: p.name.clone(), offset: (i as f64 + 1.0) / (k.inputs.len() as f64 + 1.0), class: format!("mk-flow-handle-in mk-port-{}", p.name) }
                    }
                    for (i, p) in k.outputs.iter().enumerate() {
                        Handle { kind: HandleKind::Source, position: Side::Right, id: p.name.clone(), offset: (i as f64 + 1.0) / (k.outputs.len() as f64 + 1.0), class: format!("mk-flow-handle-out mk-port-{}", p.name) }
                    }
                    div { class: "mk-flow-ports",
                        div { class: "mk-flow-ports-in", for p in k.inputs.iter() { div { key: "{p.name}", title: "{p.ty.describe()}", "◂ {p.name}" } } }
                        div { class: "mk-flow-ports-out", for p in k.outputs.iter() { div { key: "{p.name}", title: "{p.ty.describe()}", "{p.name} ▸" } } }
                    }
                    if !k.params.is_empty() {
                        // Keys typed into a parameter field must not reach the
                        // node/canvas handlers (Delete/Backspace remove the block,
                        // arrows nudge it, Enter/Space toggle selection); Ctrl/Meta
                        // combos (Ctrl+S) still bubble. Pointer-down likewise, or a
                        // click into the field starts a drag (spec 001).
                        div { class: "mk-flow-params",
                            onkeydown: move |e| {
                                if !(e.modifiers().ctrl() || e.modifiers().meta()) {
                                    e.stop_propagation();
                                }
                            },
                            onpointerdown: |e| e.stop_propagation(),
                            for p in k.params.iter() {
                                {
                                    let value = n.data.params.get(&p.name).cloned().unwrap_or_else(|| p.default.clone());
                                    let (id2, name) = (id.clone(), p.name.clone());
                                    let mut set_param = set_param.clone();
                                    rsx! {
                                        label { key: "{p.name}", class: "mk-flow-param",
                                            span { "{p.name}" }
                                            match &p.kind {
                                                ParamKind::Choice { options } => rsx! {
                                                    select { value: "{value}", onmousedown: |e| e.stop_propagation(),
                                                        onchange: move |e| set_param((id2.clone(), name.clone(), e.value())),
                                                        for o in options.iter() { option { value: "{o}", selected: *o == value, "{o}" } }
                                                    }
                                                },
                                                ParamKind::Bool => rsx! {
                                                    input { r#type: "checkbox", checked: value == "true", onmousedown: |e| e.stop_propagation(),
                                                        onchange: move |e| set_param((id2.clone(), name.clone(), e.checked().to_string())) }
                                                },
                                                _ => rsx! {
                                                    input { value: "{value}", size: 6, onmousedown: |e| e.stop_propagation(),
                                                        onchange: move |e| set_param((id2.clone(), name.clone(), e.value())) }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "mk-flow-block-missing", "unknown kind {n.data.kind}" }
                }
            }
        }
    });

    let dirty = doc.read().dirty();
    let flow_now = to_flow(&nodes.read(), &edges.read());
    let issues = validate(&flow_now, &libs);
    let save = move |_| {
        spawn(async move {
            match ws.save(node).await {
                Ok(()) => error.set(None),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    // Codegen: every library with a generator and at least one block in use.
    let generators: Vec<FlowLibrary> = libs
        .iter()
        .filter(|l| {
            l.codegen.is_some()
                && flow_now
                    .blocks
                    .iter()
                    .any(|b| b.kind.starts_with(&format!("{}/", l.id)))
        })
        .cloned()
        .collect();
    let generate = move |lib: FlowLibrary| {
        let flow = to_flow(&nodes.peek(), &edges.peek());
        spawn(async move {
            let Some(gen) = lib.codegen else { return };
            match gen(&flow, &lib) {
                Ok(out) => {
                    let (source, dir) = {
                        let d = doc.peek();
                        let dir = d
                            .node
                            .native_key
                            .rsplit_once('/')
                            .map(|(d, _)| d.to_string())
                            .unwrap_or_default();
                        (d.node.source.clone(), dir)
                    };
                    let rel = if dir.is_empty() {
                        out.file_name.clone()
                    } else {
                        format!("{dir}/{}", out.file_name)
                    };
                    let mut ws = ws;
                    // Overwrite if it exists, else create.
                    let result = match ws.node_at_path(&source, &rel).await {
                        Some(existing) => {
                            let Some(src) = ws.source(&source) else {
                                return;
                            };
                            let chars = src
                                .fetch_text(existing.id)
                                .await
                                .map(|(t, _)| t.chars().count())
                                .unwrap_or(0);
                            src.apply(moonkale_core::Transaction::write_text(
                                existing.id,
                                existing.version,
                                moonkale_core::TextPatch::whole(&out.text, chars),
                            ))
                            .await
                            .map(|_| existing)
                            .map_err(|e| e.to_string())
                        }
                        None => {
                            let root = ws.source(&source).map(|s| s.descriptor().root);
                            match root {
                                Some(root) => ws
                                    .create_text(&source, root, &rel, &out.text)
                                    .await
                                    .map_err(|e| e.to_string()),
                                None => Err("source gone".into()),
                            }
                        }
                    };
                    match result {
                        Ok(n) => {
                            let _ = ws.reload(n.id).await;
                            let _ = ws.open_node(n).await;
                            ws.set_status(format!("Generated {rel} — run: {}", out.run_hint));
                        }
                        Err(e) => error.set(Some(format!("codegen: {e}"))),
                    }
                }
                Err(e) => error.set(Some(format!("codegen: {e}"))),
            }
        });
    };

    rsx! {
        document::Stylesheet { href: CSS }
        div {
            class: "mk-flow",
            onkeydown: move |e| {
                if (e.modifiers().ctrl() || e.modifiers().meta()) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    save(());
                }
            },
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{doc.read().node.native_key}" }
                if dirty { span { class: "mk-editor-dirty", title: "Unsaved changes", "●" } }
                span { class: "mk-editor-spacer" }
                span { class: "mk-flow-status", "data-blocks": "{flow_now.blocks.len()}", "data-wires": "{flow_now.wires.len()}", "data-issues": "{issues.len()}",
                    "{flow_now.blocks.len()} blocks · {flow_now.wires.len()} wires"
                    if !issues.is_empty() { span { class: "mk-flow-issues", title: "{issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>().join(\"\\n\")}", " · {issues.len()} issue(s)" } }
                }
                for lib in generators {
                    button { key: "{lib.id}", class: "mk-btn mk-flow-generate", onclick: { let lib = lib.clone(); move |_| generate(lib.clone()) }, title: "Write the {lib.language} file next to this flow", "Generate {lib.language}" }
                }
                button { class: "mk-btn", onclick: move |_| handle.auto_layout(&LayoutOptions { direction: LayoutDirection::LeftToRight, node_gap: 40.0, rank_gap: 90.0, update_handle_sides: false }), "Layout" }
                button { class: "mk-btn", onclick: move |_| handle.fit_view(200), "Fit" }
                button { class: "mk-btn", disabled: !dirty, onclick: move |_| save(()), "Save" }
            }
            if let Some(e) = error() { div { class: "mk-editor-error", "{e}" } }
            div { class: "mk-flow-body",
                div { class: "mk-flow-palette",
                    if libs.is_empty() {
                        p { class: "mk-muted", "No block libraries are enabled. Turn one on in Settings → Extensions (e.g. Lux.jl)." }
                    } else {
                        if libs.len() > 1 {
                            select { class: "mk-input", onchange: move |e| selected_lib.set(e.value().parse().unwrap_or(0)),
                                for (i, l) in libs.iter().enumerate() { option { value: "{i}", selected: i == selected_lib(), "{l.name}" } }
                            }
                        }
                        {
                            let lib = libs.get(selected_lib()).or(libs.first()).cloned();
                            rsx! {
                                if let Some(lib) = lib {
                                    div { class: "mk-flow-palette-title", "{lib.name}" }
                                    {
                                        let mut cats: Vec<&str> = lib.blocks.iter().map(|b| b.category.as_str()).collect();
                                        cats.dedup();
                                        rsx! {
                                            for cat in cats {
                                                div { key: "{cat}", class: "mk-flow-palette-cat", "{cat}" }
                                                for b in lib.blocks.iter().filter(|b| b.category == cat) {
                                                    button { key: "{b.id}", class: "mk-flow-palette-block", title: "{b.description}",
                                                        onclick: { let mut add = add_block.clone(); let kind = format!("{}/{}", lib.id, b.id); move |_| add(kind.clone()) },
                                                        "{b.name}"
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
                div { class: "mk-flow-canvas",
                    Flow {
                        nodes,
                        edges,
                        handle,
                        node_view,
                        is_valid_connection: is_valid,
                        on_connect,
                        on_node_drag_stop: on_drag_stop,
                        on_delete,
                        drag_threshold: 4.0,
                        pan_on_scroll: false,
                        Background {}
                        Controls {}
                    }
                }
            }
        }
    }
}
