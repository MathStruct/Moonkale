//! The flow editor panel: the graph engine's edit mode over one
//! `*.flow.json` document — a palette of the enabled libraries' blocks,
//! typed wiring (the engine emits `connect`; this panel validates with
//! `unify` and adds the wire — the renderer never mutates the model,
//! ADR-0018), per-block parameters in the side editor of the selected
//! block (one HTML editor, spec 031 §7 — no per-node DOM), validation and
//! codegen.
//!
//! State: the `Flow` signal is the working copy; every change is
//! serialised back into the `Document` (dirty → Save, like any editor)
//! and re-sent to the canvas, which matches by id and keeps the live
//! positions. The document is re-read only when its text is replaced from
//! outside (reload). A block's position is the document's — the canvas
//! reports `moved` and the panel harvests the coordinates back.

use crate::L;
use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use moonkale_core::NodeId as CoreNodeId;
use moonkale_ext_api::flow::{
    find_kind, validate, Block, Flow, FlowLibrary, ParamKind, PortType, Wire,
};
use moonkale_ext_api::{t, Workspace};
use moonkale_graph_assets::{RENDER_JS, RENDER_WASM};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const CSS: Asset = asset!("/assets/flow.css");

/// The engine's block half-width (world units; `RECT_WIDTH` there).
const BLOCK_W: f32 = 46.0;

/// A hue per block kind, so two libraries' blocks read apart at a glance
/// (the label-colour trick from the graph panel).
const PALETTE: [&str; 12] = [
    "#f2a03d", "#4fb3e8", "#8ccf6a", "#e86b8a", "#b48cf0", "#f0d55a", "#4fd6c2", "#f07c4f",
    "#9ab7ff", "#d69bd6", "#a8d86b", "#ff9fb3",
];

fn kind_color(kind: &str) -> &'static str {
    let h = kind
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32));
    PALETTE[(h % PALETTE.len() as u32) as usize]
}

/// A port's colour by what it carries — the types read at a glance while
/// wiring (spec 031 §2: typed-looking boundary markers).
fn port_color(ty: &PortType) -> &'static str {
    match ty {
        PortType::Any => "#8b93a5",
        PortType::Scalar => "#8ccf6a",
        PortType::Tensor { .. } => "#ffe459",
        PortType::Named { .. } => "#e86b8a",
    }
}

// ---- the engine's wire format (one conversion site, ADR-0018 §2) --------

#[derive(Serialize)]
struct OutNode<'a> {
    id: &'a str,
    label: &'a str,
    /// The block's `library/kind` (the engine's own `kind` field); the
    /// explicit `color` decides how it draws.
    kind: &'a str,
    color: &'static str,
    shape: &'static str,
    width: f32,
    height: f32,
    pos: [f32; 2],
    pinned: bool,
}

#[derive(Serialize)]
struct OutEdge<'a> {
    id: &'a str,
    a: usize,
    b: usize,
    directed: bool,
    a_port: &'a str,
    b_port: &'a str,
    route: OutRoute,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum OutRoute {
    Ortho {},
}

#[derive(Serialize)]
struct OutPort {
    node: usize,
    name: String,
    side: &'static str,
    offset: f32,
    direction: &'static str,
    color: &'static str,
}

#[derive(Serialize)]
struct OutScene<'a> {
    nodes: Vec<OutNode<'a>>,
    edges: Vec<OutEdge<'a>>,
    ports: Vec<OutPort>,
}

/// `Flow` → the engine's scene. The ids are the blocks' and wires' ids, so
/// the engine's incremental `set_graph` keeps the live positions and the
/// selection across every sync.
fn to_scene<'a>(flow: &'a Flow, libs: &'a [FlowLibrary], issues: &[Issue]) -> OutScene<'a> {
    let index: BTreeMap<&str, usize> = flow
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.as_str(), i))
        .collect();
    let mut nodes = Vec::new();
    let mut ports = Vec::new();
    for (i, b) in flow.blocks.iter().enumerate() {
        let resolved = find_kind(libs, &b.kind);
        let (name, kind_ports) = match &resolved {
            Some((_, k)) => (k.name.as_str(), Some(k)),
            None => (b.kind.as_str(), None),
        };
        let bad = issues.iter().any(|x| x.about == b.id);
        let (n_in, n_out) = kind_ports
            .map(|k| (k.inputs.len(), k.outputs.len()))
            .unwrap_or((0, 0));
        // Tall enough for its ports; a little room for the label.
        let height = (10.0 + 7.0 * n_in.max(n_out) as f32).max(22.0);
        let width = BLOCK_W.max(name.len() as f32 * 1.9);
        nodes.push(OutNode {
            id: &b.id,
            label: name,
            kind: &b.kind,
            color: if bad { "#e86b6a" } else { kind_color(&b.kind) },
            shape: "rect",
            width,
            height,
            pos: [b.x as f32, b.y as f32],
            pinned: true, // the document owns the positions
        });
        if let Some((_, k)) = resolved {
            for (j, p) in k.inputs.iter().enumerate() {
                ports.push(OutPort {
                    node: i,
                    name: p.name.clone(),
                    side: "left",
                    offset: (j + 1) as f32 / (n_in + 1) as f32,
                    direction: "in",
                    color: port_color(&p.ty),
                });
            }
            for (j, p) in k.outputs.iter().enumerate() {
                ports.push(OutPort {
                    node: i,
                    name: p.name.clone(),
                    side: "right",
                    offset: (j + 1) as f32 / (n_out + 1) as f32,
                    direction: "out",
                    color: port_color(&p.ty),
                });
            }
        }
    }
    let edges = flow
        .wires
        .iter()
        // A wire naming a gone block parses (validate reports it) — it
        // draws nothing here instead of panicking on the index.
        .filter_map(|w| {
            let (Some(&a), Some(&b)) = (
                index.get(w.from_block.as_str()),
                index.get(w.to_block.as_str()),
            ) else {
                return None;
            };
            Some(OutEdge {
                id: &w.id,
                a,
                b,
                directed: true,
                a_port: &w.from_port,
                b_port: &w.to_port,
                route: OutRoute::Ortho {},
                color: issues.iter().any(|x| x.about == w.id).then_some("#e86b6a"),
            })
        })
        .collect();
    OutScene {
        nodes,
        edges,
        ports,
    }
}

use moonkale_ext_api::flow::Issue;

// ---- the eval channel ----------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    SetGraph {
        scene: OutScene<'a>,
    },
    Fit,
    /// The layered arrangement (the Layout button); the script answers
    /// with the harvested positions, like a drag's `moved`.
    HierLayout,
    Destroy,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct End {
    node: String,
    port: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Loaded,
    Ready,
    Error {
        message: String,
    },
    Select {
        nodes: Vec<String>,
        edges: Vec<String>,
        /// The first selected node's canvas position (stage 7c): the
        /// parameter editor follows its block.
        screen: Option<Vec<f32>>,
    },
    Connect {
        from: End,
        to: End,
    },
    Moved {
        /// The nodes that moved — a drag-end names the dragged block,
        /// the Layout names every node.
        positions: BTreeMap<String, [f32; 2]>,
        /// The dragged node's screen position, so the parameter editor
        /// follows its block (stage 7c).
        screen: Option<Vec<f32>>,
    },
    Settled,
}

const SCRIPT: &str = r#"
// Handshake: messages sent before the host awaits are lost, so wait for
// the host's first message before sending anything.
await dioxus.recv();
const canvas = document.getElementById(ID + "-gl");
const overlay = document.getElementById(ID + "-ov");
if (!canvas || !overlay) { return; }
const failed = async (message) => {
    dioxus.send({ kind: "error", message });
    for (;;) { const m = await dioxus.recv(); if (m.kind === "destroy") return; }
};
let mod;
try {
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
try { probeGl && probeGl.getExtension("WEBGL_lose_context")?.loseContext(); } catch (_) {}
let view;
// Events pass through untouched: the engine builds the whole payload
// (a `moved` carries its positions, a `select` the first node's screen)
// because the host cannot call back into the view from inside an event
// — the engine holds its state borrow while emitting.
const onEvent = (ev) => dioxus.send(ev);
try {
    const timeout = new Promise((_, rej) => setTimeout(() => rej(new Error("renderer did not start within 15s")), 15000));
    const prefer = /\bAndroid\b/.test(navigator.userAgent) && /\bwv\b/.test(navigator.userAgent) ? "gl" : null;
    view = await Promise.race([mod.create(canvas, overlay, onEvent, prefer), timeout]);
} catch (e) {
    return failed(String(e && e.message ? e.message : e));
}
view.set_interaction("edit"); // the flow surface is always an edit surface
const host = canvas.parentElement;
const ro = new ResizeObserver(() => {
    const r = host.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) view.resize(r.width, r.height, window.devicePixelRatio || 1);
});
ro.observe(host);
window.moonkale = window.moonkale || {};
(window.moonkale.flowViews = window.moonkale.flowViews || {})[ID] = view;
const retheme = () => {
    const css = getComputedStyle(host);
    const v = (n) => css.getPropertyValue(n).trim();
    try { view.set_theme(v("--mk-graph-bg"), v("--mk-graph-label"), v("--mk-graph-label-hover")); } catch (_) {}
};
retheme();
const themeObserver = new MutationObserver(retheme);
themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
window.addEventListener("moonkale-theme", retheme);
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setGraph") view.set_graph(JSON.stringify(msg.scene));
    else if (msg.kind === "fit") view.fit();
    else if (msg.kind === "hierLayout") {
        view.hier_layout();
        // `node_positions()` answers a JSON string; the host expects a map.
        dioxus.send({ kind: "moved", positions: JSON.parse(view.node_positions()) });
    }
    else if (msg.kind === "destroy") {
        ro.disconnect(); themeObserver.disconnect(); window.removeEventListener("moonkale-theme", retheme);
        view.destroy(); delete window.moonkale.flowViews[ID]; break;
    }
}
"#;

#[component]
pub fn FlowPanel(ws: Workspace, node: CoreNodeId) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let libs = ws.contrib.flow_libraries.read().clone();
    // A document that does not parse is shown as an error and never written
    // back: an empty canvas synced into it would replace the file (#11).
    let parsed = Flow::parse(&doc.peek().text);
    let mut broken: Signal<Option<String>> = use_signal(|| parsed.as_ref().err().cloned());
    let initial = parsed.unwrap_or_default();
    let mut flow: Signal<Flow> = use_signal(|| initial);
    let mut last_text = use_signal(|| doc.peek().text.clone());
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut notice: Signal<Option<String>> = use_signal(|| None);
    let mut selected_lib = use_signal(|| 0usize);
    let mut selected: Signal<(Vec<String>, Vec<String>)> = use_signal(|| (Vec::new(), Vec::new()));
    let mut eval: Signal<Option<Eval>> = use_signal(|| None);
    let mut loaded = use_signal(|| false);
    // Undo (stage 7c, the parity list): the working copy before each
    // mutation, newest last; a new mutation empties the redo stack.
    // Snapshots are the document's own JSON — restoring one re-sends the
    // scene and the engine moves the pinned blocks back (their
    // coordinates are the document's).
    let mut undo_stack: Signal<Vec<String>> = use_signal(Vec::new);
    let mut redo_stack: Signal<Vec<String>> = use_signal(Vec::new);
    // Where the selected block sits on the canvas (the editor follows).
    let mut editor_pos: Signal<Option<(f32, f32)>> = use_signal(|| None);
    // One DOM id per panel: two split views show two flows at once, and
    // the canvas ids and `flowViews` key derive from it — a fixed id
    // made the second view draw into the first's canvas (the review's #6).
    let id = use_hook(|| format!("mk-flow-{node}"));

    // Mount the renderer from the host element's `onmounted` event.
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
                        Ok(FromJs::Ready) => {}
                        Ok(FromJs::Error { message }) => error.set(Some(message)),
                        Ok(FromJs::Select {
                            nodes,
                            edges,
                            screen,
                        }) => {
                            editor_pos.set(screen.filter(|s| s.len() == 2).map(|s| (s[0], s[1])));
                            selected.set((nodes, edges))
                        }
                        Ok(FromJs::Connect { from, to }) => {
                            // A connect that races a reload into a broken
                            // document is refused — the model never takes
                            // a wire it cannot write back (#11).
                            if broken.peek().is_some() {
                                continue;
                            }
                            // The wire types are checked against the
                            // libraries as they are *now* — a library
                            // enabled after this loop started must be
                            // found, and one disabled must not lend its
                            // ports to a connect (the review's #7).
                            let libs = ws.contrib.flow_libraries.read().clone();
                            let before = flow.read().to_json();
                            let (ok, why) = wire(&mut flow, &libs, &from, &to);
                            if ok {
                                push_undo(undo_stack, redo_stack, before);
                                notice.set(None);
                                sync(&flow, &mut last_text, &mut doc, &broken);
                            } else {
                                // The gesture drew a wire; the model did
                                // not take it. The engine drops its own
                                // gesture wire (`end_wire`) — the canvas
                                // reverts without a scene push.
                                notice.set(Some(why));
                            }
                        }
                        Ok(FromJs::Moved { positions, screen }) => {
                            // The Layout reply (and a drag) never lands in
                            // a broken document (#11, the review's #3).
                            if broken.peek().is_some() {
                                continue;
                            }
                            let before = flow.read().to_json();
                            flow.with_mut(|f| {
                                for b in &mut f.blocks {
                                    if let Some(p) = positions.get(&b.id) {
                                        b.x = p[0] as f64;
                                        b.y = p[1] as f64;
                                    }
                                }
                            });
                            push_undo(undo_stack, redo_stack, before);
                            // The editor follows a dragged selected block.
                            let sel = selected.read().0.first().cloned();
                            if let (Some(sel), Some(p)) = (sel, screen) {
                                if positions.contains_key(&sel) && p.len() == 2 {
                                    editor_pos.set(Some((p[0], p[1])));
                                }
                            }
                            sync(&flow, &mut last_text, &mut doc, &broken);
                        }
                        Ok(FromJs::Settled) => {}
                        Err(dioxus::document::EvalError::Serialization(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            let _ = ev.send(serde_json::json!({ "kind": "init" }));
            eval.set(Some(ev));
        }
    };
    use_drop(move || {
        if let Some(ev) = eval.peek().as_ref() {
            let _ = ev.send(ToJs::Destroy);
        }
    });

    // The scene goes out when the canvas is up, and again on every flow
    // or library change — the engine matches by id and keeps the live
    // positions, so nothing else is re-sent. This effect is the single
    // push path: the mutators below change `flow` and never push their
    // own scene (the review's #10 — every edit used to rebuild twice).
    // Reading the libraries here (not a copy from mount) also re-sends
    // when a library is enabled or disabled, so the ports follow (#7).
    // A scene that has grown beyond the fitted box refits — the
    // palette's grid and the Layout both reach past the default camera —
    // but wires and drags that stay inside never move the viewport
    // under the user.
    let eval_for_scene = eval;
    let loaded_for_scene = loaded;
    let mut fitted: Signal<Option<[f64; 4]>> = use_signal(|| None);
    use_effect(move || {
        if loaded_for_scene() {
            let libs = ws.contrib.flow_libraries.read().clone();
            push_graph(&eval_for_scene, &flow, &libs);
            let bounds = flow
                .read()
                .blocks
                .iter()
                .fold(None::<[f64; 4]>, |a, b| match a {
                    None => Some([b.x, b.y, b.x, b.y]),
                    Some([x0, y0, x1, y1]) => {
                        Some([x0.min(b.x), y0.min(b.y), x1.max(b.x), y1.max(b.y)])
                    }
                });
            let refit = match (bounds, fitted()) {
                (Some(b), Some(f)) => b[0] < f[0] || b[1] < f[1] || b[2] > f[2] || b[3] > f[3],
                (Some(_), None) => true,
                _ => false,
            };
            if refit {
                fitted.set(bounds);
                if let Some(ev) = eval_for_scene.peek().as_ref() {
                    let _ = ev.send(ToJs::Fit);
                }
            }
        }
    });

    // Document replaced from outside (reload): rebuild the flow. The
    // snapshots describe the previous document — both stacks empty.
    {
        use_effect(move || {
            let text = doc.read().text.clone();
            if text == *last_text.peek() {
                return;
            }
            last_text.set(text.clone());
            undo_stack.write().clear();
            redo_stack.write().clear();
            editor_pos.set(None);
            // The selection names blocks of the *old* document; keeping it
            // let Delete act on a stale view (#11, the review's #3).
            selected.set((Vec::new(), Vec::new()));
            match Flow::parse(&text) {
                Ok(f) => {
                    broken.set(None);
                    flow.set(f);
                }
                Err(e) => broken.set(Some(e)),
            }
        });
    }

    // Going broken unmounts the canvas div; coming back mounts a *new*
    // div. The old renderer still points at the old elements, and the
    // mount guard would refuse a second init — the new canvas would stay
    // blank (the review's #5). Tear the eval down on the way into
    // broken; the fresh div then mounts a fresh renderer.
    {
        use_effect(move || {
            if broken().is_some() {
                if let Some(ev) = eval.peek().as_ref() {
                    let _ = ev.send(ToJs::Destroy);
                }
                eval.set(None);
                loaded.set(false);
                fitted.set(None);
            }
        });
    }

    let issues_now = validate(&flow.read(), &libs);
    let dirty = doc.read().dirty();
    let save = move |_| {
        spawn(async move {
            match ws.save(node).await {
                Ok(()) => error.set(None),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    // Codegen: every library with a generator and at least one block in use.
    let flow_for_gen = flow.read().clone();
    let generators: Vec<FlowLibrary> = libs
        .iter()
        .filter(|_| broken.read().is_none())
        .filter(|l| {
            l.codegen.is_some()
                && flow_for_gen
                    .blocks
                    .iter()
                    .any(|b| b.kind.starts_with(&format!("{}/", l.id)))
        })
        .cloned()
        .collect();
    let generate = move |lib: FlowLibrary| {
        let flow = flow.read().clone();
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
                            ws.set_status(t!(
                                ws,
                                L,
                                "flow-generated",
                                file = rel.clone(),
                                run = out.run_hint.clone()
                            ));
                        }
                        Err(e) => error.set(Some(format!("codegen: {e}"))),
                    }
                }
                Err(e) => error.set(Some(format!("codegen: {e}"))),
            }
        });
    };

    // A palette pick adds a block on a 3-column grid, as before. The
    // kinds resolve against the libraries as they are now, so a library
    // enabled after the panel opened is addable (the review's #7).
    let add_block = move |kind: String| {
        // Never against a broken document (#11): the model must not
        // hold an edit it refuses to write back.
        if broken.peek().is_some() {
            return;
        }
        let libs = ws.contrib.flow_libraries.read().clone();
        let Some((lib, k)) = find_kind(&libs, &kind) else {
            return;
        };
        let n = flow.read().blocks.len();
        let (col, row) = ((n % 3) as f64, (n / 3) as f64);
        let params = k
            .params
            .iter()
            .map(|p| (p.name.clone(), p.default.clone()))
            .collect();
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            f.blocks.push(Block {
                id: format!(
                    "{}-{}",
                    k.id,
                    &uuid::Uuid::new_v4().simple().to_string()[..6]
                ),
                kind: format!("{}/{}", lib.id, k.id),
                x: 30.0 + col * 165.0,
                y: 30.0 + row * 170.0,
                params,
            });
        });
        push_undo(undo_stack, redo_stack, before);
        notice.set(None);
        sync(&flow, &mut last_text, &mut doc, &broken);
        // No scene push — the flow change re-runs the scene effect.
    };

    // The parameter editor writes one parameter of the selected block.
    let set_param = move |(bid, name, value): (String, String, String)| {
        if broken.peek().is_some() {
            return; // #11: a broken document is never written
        }
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            if let Some(b) = f.blocks.iter_mut().find(|b| b.id == bid) {
                b.params.insert(name, value);
            }
        });
        push_undo(undo_stack, redo_stack, before);
        sync(&flow, &mut last_text, &mut doc, &broken);
    };

    // Delete removes the selected blocks and wires (the engine only
    // reports the selection; the model is the panel's to mutate).
    let mut delete_selected = move |_| {
        // #11: a stale selection against a broken document must not
        // reach the file — this is the exact path that lost data.
        if broken.peek().is_some() {
            return;
        }
        let (nodes, wires) = selected.read().clone();
        if nodes.is_empty() && wires.is_empty() {
            return;
        }
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            f.blocks.retain(|b| !nodes.contains(&b.id));
            f.wires.retain(|w| {
                !wires.contains(&w.id)
                    && !nodes.contains(&w.from_block)
                    && !nodes.contains(&w.to_block)
            });
        });
        push_undo(undo_stack, redo_stack, before);
        selected.set((Vec::new(), Vec::new()));
        editor_pos.set(None);
        notice.set(None);
        sync(&flow, &mut last_text, &mut doc, &broken);
    };

    // Undo restores the last snapshot (Ctrl+Z), redo the one undone
    // (Ctrl+Y / Ctrl+Shift+Z). A restore changes `flow`, so the scene
    // effect re-sends it; the engine moves the pinned blocks to the
    // restored coordinates.
    let mut undo = move |_| {
        if broken.peek().is_some() {
            return; // #11: a broken document is never written
        }
        let Some(prev) = undo_stack.read().last().cloned() else {
            return;
        };
        let cur = flow.read().to_json();
        let Ok(f) = Flow::parse(&prev) else {
            return;
        };
        undo_stack.with_mut(|u| {
            u.pop();
        });
        redo_stack.with_mut(|r| r.push(cur));
        flow.set(f);
        selected.set((Vec::new(), Vec::new()));
        editor_pos.set(None);
        notice.set(None);
        sync(&flow, &mut last_text, &mut doc, &broken);
    };
    let mut redo = move |_| {
        if broken.peek().is_some() {
            return; // #11: a broken document is never written
        }
        let Some(next) = redo_stack.read().last().cloned() else {
            return;
        };
        let cur = flow.read().to_json();
        let Ok(f) = Flow::parse(&next) else {
            return;
        };
        redo_stack.with_mut(|r| {
            r.pop();
        });
        undo_stack.with_mut(|u| u.push(cur));
        flow.set(f);
        selected.set((Vec::new(), Vec::new()));
        editor_pos.set(None);
        notice.set(None);
        sync(&flow, &mut last_text, &mut doc, &broken);
    };

    // The selected block's parameters (spec 031 §7: one editor for the
    // selected node, not a component per block).
    let (sel_blocks, _sel_wires) = selected.read().clone();
    let sel_block = sel_blocks
        .first()
        .and_then(|id| flow.read().block(id).cloned());
    let sel_kind = sel_block
        .as_ref()
        .and_then(|b| find_kind(&libs, &b.kind).map(|(_, k)| k.clone()));

    rsx! {
        moonkale_ext_api::Stylesheet { href: CSS }
        div {
            class: "mk-flow",
            // Delete/Backspace remove the selection — never while a field
            // has the focus (spec 001); Ctrl/Meta combos bubble (Ctrl+S).
            // Undo (Ctrl+Z) and redo (Ctrl+Y, Ctrl+Shift+Z) restore the
            // last snapshot; the field editor stops its own Z/Y (below).
            onkeydown: move |e| {
                if moonkale_ext_api::keys::primary(&e.modifiers())
                    && e.key() == Key::Character("s".into())
                {
                    e.prevent_default();
                    e.stop_propagation();
                    save(());
                } else if moonkale_ext_api::keys::primary(&e.modifiers()) {
                    let shift = e.modifiers().shift();
                    match e.key() {
                        Key::Character(c) if c.eq_ignore_ascii_case("z") => {
                            e.prevent_default();
                            e.stop_propagation();
                            if shift { redo(()) } else { undo(()) }
                        }
                        Key::Character(c) if c.eq_ignore_ascii_case("y") => {
                            e.prevent_default();
                            e.stop_propagation();
                            redo(());
                        }
                        _ => {}
                    }
                } else if matches!(e.key(), Key::Delete | Key::Backspace) {
                    // The editor stops its own keys (below), so a key
                    // that reaches the root is aimed at the canvas.
                    e.prevent_default();
                    delete_selected(());
                }
            },
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{doc.read().node.native_key}" }
                if dirty { span { class: "mk-editor-dirty", title: t!(ws, L, "flow-unsaved"), "●" } }
                span { class: "mk-editor-spacer" }
                if let Some(n) = notice() { span { class: "mk-flow-notice", "{n}" } }
                span { class: "mk-flow-status", "data-blocks": "{flow.read().blocks.len()}", "data-wires": "{flow.read().wires.len()}", "data-issues": "{issues_now.len()}",
                    {t!(ws, L, "flow-counts", blocks = flow.read().blocks.len(), wires = flow.read().wires.len())}
                    if !issues_now.is_empty() { span { class: "mk-flow-issues", title: "{issues_now.iter().map(|i| i.message.clone()).collect::<Vec<_>>().join(\"\\n\")}", {t!(ws, L, "flow-issues", n = issues_now.len())} } }
                }
                for lib in generators {
                    button { key: "{lib.id}", class: "mk-btn mk-flow-generate", onclick: { let lib = lib.clone(); move |_| generate(lib.clone()) }, title: t!(ws, L, "flow-generate-title", language = lib.language), {t!(ws, L, "flow-generate", language = lib.language)} }
                }
                // While broken there is nothing to arrange, fit or save —
                // only Reload leads out (the review's #3: the toolbar
                // must not offer edits the model refuses to write).
                button { class: "mk-btn", disabled: broken.read().is_some(), onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::HierLayout); } }, {t!(ws, L, "flow-layout")} }
                button { class: "mk-btn", disabled: broken.read().is_some(), onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Fit); } }, {t!(ws, L, "flow-fit")} }
                button { class: "mk-btn", disabled: !dirty || broken.read().is_some(), onclick: move |_| save(()), {t!(ws, L, "flow-save")} }
                button { class: "mk-btn mk-flow-reload", title: t!(ws, L, "flow-reload-title"),
                    onclick: move |_| { spawn(async move { if let Err(e) = ws.reload(node).await { error.set(Some(e.to_string())); } }); },
                    {t!(ws, L, "flow-reload")}
                }
            }
            if let Some(e) = error() { div { class: "mk-editor-error", "{e}" } }
            if let Some(why) = broken() {
                div { class: "mk-editor-error mk-flow-broken", role: "alert", {t!(ws, L, "flow-broken", error = why)} }
            } else {
                div { class: "mk-flow-body",
                    // Each palette button carries its own handler with
                    // the kind captured in the closure — no DOM reading,
                    // which on desktop and Android (native handlers,
                    // outside wasm) crashed on `web_sys::window()` (the
                    // review's #1). The buttons are keyed: the delegated
                    // double fire of the stage 7b quirk (P-167)
                    // came from unkeyed list items borrowing the first
                    // item's listener.
                    div {
                        class: "mk-flow-palette",
                        if libs.is_empty() {
                            p { class: "mk-muted", {t!(ws, L, "flow-no-libraries")} }
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
                                                        {
                                                            let kind = format!("{}/{}", lib.id, b.id);
                                                            rsx! {
                                                                button { key: "{kind}",
                                                                    "data-kind": "{kind}",
                                                                    class: "mk-flow-palette-block",
                                                                    title: "{b.description}",
                                                                    onclick: {
                                                                        let mut add_block = add_block;
                                                                        let kind = kind.clone();
                                                                        move |_| add_block(kind.clone())
                                                                    },
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
                        }
                    }
                    div {
                        class: "mk-flow-canvas",
                        onmounted: { let mut mount = mount; move |e| mount(e) },
                        canvas { id: "{id}-gl", class: "mk-flow-gl" }
                        // The overlay is where clicks land (it sits on
                        // top) and takes the focus for the panel's keys.
                        canvas { id: "{id}-ov", class: "mk-flow-ov", tabindex: "0" }
                        // One HTML editor for the selected node (spec 031
                        // §7), floating beside its block on the canvas —
                        // positioned from the engine's screen report, so it
                        // follows the block (select, drag; a pan or zoom
                        // while it is open waits for the next event).
                        if let (Some(b), Some(k)) = (sel_block.as_ref(), sel_kind.as_ref()) {
                            div {
                                class: "mk-flow-props",
                                style: match editor_pos() {
                                    Some((x, y)) => {
                                        format!("left: {:.0}px; top: {:.0}px;", (x + 60.0).max(6.0), (y - 18.0).max(6.0))
                                    }
                                    None => "display: none;".to_string(),
                                },
                                // Keys typed into a parameter field must not
                                // reach the panel (Delete/Backspace remove the
                                // selection); nor do the field's own Z/Y — a
                                // field's Ctrl+Z is its text undo. Ctrl/Meta
                                // combos (Ctrl+S) still bubble (spec 001).
                                onkeydown: move |e| {
                                    let primary = moonkale_ext_api::keys::primary(&e.modifiers());
                                    let field_undo = primary
                                        && matches!(e.key(), Key::Character(ref c)
                                            if c.eq_ignore_ascii_case("z") || c.eq_ignore_ascii_case("y"));
                                    if !primary || field_undo {
                                        e.stop_propagation();
                                    }
                                },
                                div { class: "mk-flow-props-title", "{b.id}" }
                                for p in k.params.iter() {
                                    {
                                        let value = b.params.get(&p.name).cloned().unwrap_or_else(|| p.default.clone());
                                        let (bid, name) = (b.id.clone(), p.name.clone());
                                        let mut set_param = set_param;
                                        rsx! {
                                            label { key: "{p.name}", class: "mk-flow-param",
                                                span { "{p.name}" }
                                                match &p.kind {
                                                    ParamKind::Choice { options } => rsx! {
                                                        select { value: "{value}",
                                                            onchange: move |e| set_param((bid.clone(), name.clone(), e.value())),
                                                            for o in options.iter() { option { value: "{o}", selected: *o == value, "{o}" } }
                                                        }
                                                    },
                                                    ParamKind::Bool => rsx! {
                                                        input { r#type: "checkbox", checked: value == "true",
                                                            onchange: move |e| set_param((bid.clone(), name.clone(), e.checked().to_string())) }
                                                    },
                                                    _ => rsx! {
                                                        input { value: "{value}", size: 6,
                                                            onchange: move |e| set_param((bid.clone(), name.clone(), e.value())) }
                                                    },
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

/// Remember the pre-mutation copy on the undo stack (capped; a new action
/// empties the redo stack) — stage 7c's parity item.
fn push_undo(
    mut undo_stack: Signal<Vec<String>>,
    mut redo_stack: Signal<Vec<String>>,
    before: String,
) {
    undo_stack.with_mut(|u| {
        u.push(before);
        let len = u.len();
        if len > 100 {
            u.drain(..len - 100);
        }
    });
    redo_stack.write().clear();
}

/// Serialise the working copy into the document (dirty → Save). A broken
/// document is never written (#11): the panel may hold edits made before
/// the reload, and writing them would replace the file's text.
fn sync(
    flow: &Signal<Flow>,
    last_text: &mut Signal<String>,
    doc: &mut Signal<moonkale_ext_api::Document>,
    broken: &Signal<Option<String>>,
) {
    if broken.peek().is_some() {
        return;
    }
    let f = flow.read();
    let text = f.to_json();
    last_text.set(text.clone());
    doc.with_mut(|d| d.text = text);
}

/// Re-send the scene after a mutation (the engine matches by id).
fn push_graph(eval: &Signal<Option<Eval>>, flow: &Signal<Flow>, libs: &[FlowLibrary]) {
    if let Some(ev) = eval.peek().as_ref() {
        let f = flow.read();
        let scene = to_scene(&f, libs, &validate(&f, libs));
        let _ = ev.send(ToJs::SetGraph { scene });
    }
}

/// One wire per input port (a new one replaces the old), and only when
/// the types unify — the panel owns the model; the renderer never does.
fn wire(flow: &mut Signal<Flow>, libs: &[FlowLibrary], from: &End, to: &End) -> (bool, String) {
    // The type check reads; the write mutates — the two must not overlap.
    let checked = {
        let f = flow.read();
        let kind_of = |id: &str| f.block(id).map(|b| b.kind.clone());
        let (Some(fk), Some(tk)) = (kind_of(&from.node), kind_of(&to.node)) else {
            return (false, "wire: unknown block".into());
        };
        let (Some((_, fkind)), Some((_, tkind))) = (find_kind(libs, &fk), find_kind(libs, &tk))
        else {
            return (false, "wire: unknown block kind".into());
        };
        let out = fkind.outputs.iter().find(|p| p.name == from.port);
        let inp = tkind.inputs.iter().find(|p| p.name == to.port);
        let (Some(o), Some(i)) = (out, inp) else {
            return (false, "wire: the port does not exist".into());
        };
        match o.ty.unify(&i.ty) {
            Some(_) => Ok(()),
            None => Err(format!(
                "{}.{} ({}) does not fit {}.{} ({})",
                from.node,
                from.port,
                o.ty.describe(),
                to.node,
                to.port,
                i.ty.describe()
            )),
        }
    };
    match checked {
        Ok(()) => {
            flow.with_mut(|f| {
                f.wires
                    .retain(|w| !(w.to_block == to.node && w.to_port == to.port));
                f.wires.push(Wire {
                    id: format!("w-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
                    from_block: from.node.clone(),
                    from_port: from.port.clone(),
                    to_block: to.node.clone(),
                    to_port: to.port.clone(),
                });
            });
            (true, String::new())
        }
        Err(why) => (false, why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.flow.json` whose wire names a deleted block parses — `validate`
    /// reports it — and must not panic the scene build (the review's #2).
    #[test]
    fn to_scene_skips_a_wire_whose_block_is_gone() {
        let flow = Flow::parse(
            r#"{"version":1,"blocks":[
                {"id":"a","kind":"lux/Input","x":0,"y":0,"params":{}},
                {"id":"b","kind":"lux/Dense","x":10,"y":10,"params":{}}],
            "wires":[{"id":"w1","from_block":"a","from_port":"out",
                      "to_block":"ghost","to_port":"in"}]}"#,
        )
        .expect("parses");
        let scene = to_scene(&flow, &[], &[]);
        assert_eq!(scene.nodes.len(), 2);
        assert!(
            scene.edges.is_empty(),
            "the dangling wire draws nothing instead of panicking"
        );

        // The wire to an existing block still draws.
        let flow = Flow::parse(
            r#"{"version":1,"blocks":[
                {"id":"a","kind":"lux/Input","x":0,"y":0,"params":{}},
                {"id":"b","kind":"lux/Dense","x":10,"y":10,"params":{}}],
            "wires":[{"id":"w1","from_block":"a","from_port":"out",
                      "to_block":"b","to_port":"in"}]}"#,
        )
        .expect("parses");
        let scene = to_scene(&flow, &[], &[]);
        assert_eq!(scene.edges.len(), 1);
        assert_eq!(scene.edges[0].a, 0);
        assert_eq!(scene.edges[0].b, 1);
    }
}
