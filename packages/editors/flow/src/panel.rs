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
    bipartite_refusal, channel_refusal, find_kind, inner_ports, validate, Block, BlockPorts, Flow,
    FlowLibrary, ParamKind, Port, PortType, Role, Subflow, Wire, BOUNDARY,
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
    id: String,
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
    /// The kind's schematic (ADR-0024): SVG path data in the block's box.
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<&'a str>,
    /// Quarter turns clockwise and the mirror (ADR-0024 §5).
    turn: u8,
    mirror: bool,
    /// The filled caption line (ADR-0024 §6).
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
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
/// selection across every sync. The root scope; an entered subflow draws
/// through [`subflow_scene`] — the engine never learns which level it
/// holds (ADR-0021 §4).
fn to_scene<'a>(flow: &'a Flow, libs: &'a [FlowLibrary], issues: &[Issue]) -> OutScene<'a> {
    let mut b = SceneBuilder::default();
    for block in flow.blocks.iter() {
        let resolved = flow.ports_of(libs, block);
        let bad = issues.iter().any(|x| x.about == block.id);
        b.push(BlockOut {
            id: block.id.clone(),
            name: display_name(block, resolved.as_ref()),
            kind: if block.subflow.is_some() {
                "subflow"
            } else {
                block.kind.as_str()
            },
            color: if bad {
                "#e86b6a"
            } else if block.subflow.is_some() {
                "#b48cf0"
            } else {
                kind_color(&block.kind)
            },
            pos: [block.x as f32, block.y as f32],
            turn: block.turn,
            mirror: block.mirror,
            caption: caption_for(
                block,
                resolved.as_ref(),
                &flow.blocks,
                &flow.wires,
                flow,
                libs,
            ),
            resolved,
        });
    }
    b.edges(
        &flow.wires,
        &|id, port| {
            flow.blocks
                .iter()
                .find(|x| x.id == id)
                .and_then(|x| flow.ports_of(libs, x))
                .is_some_and(|p| p.connections.iter().any(|p| p.name == port))
        },
        issues,
        "",
    )
}

/// An entered subflow's scene (ADR-0021 §3-4): its own blocks and wires,
/// plus one pseudo-block per declared boundary port — a declared input
/// on the left edge (a source inside), an output on the right (a
/// sink), a pin on the top — placed by the panel, never stored. The
/// declaration is the truth; the pseudo-block is the illusion.
fn subflow_scene<'a>(
    flow: &'a Flow,
    sub: &'a Subflow,
    libs: &'a [FlowLibrary],
    issues: &[Issue],
) -> OutScene<'a> {
    let mut b = SceneBuilder::default();
    let boundary_of = |p: &Port| format!("{BOUNDARY}{}", p.name);
    for (j, p) in sub.inputs.iter().enumerate() {
        b.push(BlockOut {
            id: boundary_of(p),
            name: p.name.as_str(),
            kind: "boundary/in",
            color: "#9ab7ff",
            pos: [30.0, 60.0 + 80.0 * j as f32],
            turn: 0,
            mirror: false,
            caption: None,
            resolved: Some(BlockPorts {
                name: &p.name,
                inputs: &[],
                outputs: std::slice::from_ref(p),
                connections: &[],
                params: &[],
                role: None,
                symbol: None,
                caption: None,
            }),
        });
    }
    for (j, p) in sub.outputs.iter().enumerate() {
        b.push(BlockOut {
            id: boundary_of(p),
            name: p.name.as_str(),
            kind: "boundary/out",
            color: "#9ab7ff",
            pos: [630.0, 60.0 + 80.0 * j as f32],
            turn: 0,
            mirror: false,
            caption: None,
            resolved: Some(BlockPorts {
                name: &p.name,
                inputs: std::slice::from_ref(p),
                outputs: &[],
                connections: &[],
                params: &[],
                role: None,
                symbol: None,
                caption: None,
            }),
        });
    }
    for (j, p) in sub.connections.iter().enumerate() {
        b.push(BlockOut {
            id: boundary_of(p),
            name: p.name.as_str(),
            kind: "boundary/pin",
            color: "#9ab7ff",
            pos: [330.0, 40.0 + 70.0 * j as f32],
            turn: 0,
            mirror: false,
            caption: None,
            resolved: Some(BlockPorts {
                name: &p.name,
                inputs: &[],
                outputs: &[],
                connections: std::slice::from_ref(p),
                params: &[],
                role: None,
                symbol: None,
                caption: None,
            }),
        });
    }
    for block in sub.blocks.iter() {
        let resolved = flow.ports_of(libs, block);
        let bad = issues
            .iter()
            .any(|x| x.about == format!("{}/{}", sub.id, block.id));
        b.push(BlockOut {
            id: block.id.clone(),
            name: display_name(block, resolved.as_ref()),
            kind: if block.subflow.is_some() {
                "subflow"
            } else {
                block.kind.as_str()
            },
            color: if bad {
                "#e86b6a"
            } else if block.subflow.is_some() {
                "#b48cf0"
            } else {
                kind_color(&block.kind)
            },
            pos: [block.x as f32, block.y as f32],
            turn: block.turn,
            mirror: block.mirror,
            caption: caption_for(
                block,
                resolved.as_ref(),
                &sub.blocks,
                &sub.wires,
                flow,
                libs,
            ),
            resolved,
        });
    }
    b.edges(
        &sub.wires,
        &|id, port| {
            if let Some(name) = id.strip_prefix(BOUNDARY) {
                sub.connections
                    .iter()
                    .any(|p| p.name == name && p.name == port)
            } else {
                sub.blocks
                    .iter()
                    .find(|x| x.id == id)
                    .and_then(|x| flow.ports_of(libs, x))
                    .is_some_and(|p| p.connections.iter().any(|p| p.name == port))
            }
        },
        issues,
        &sub.id,
    )
}

/// The name a block shows: a factor graph's variable its own (θ, y),
/// anything else its kind's (ADR-0022).
fn display_name<'a>(block: &'a Block, resolved: Option<&BlockPorts<'a>>) -> &'a str {
    match block.params.get("name") {
        Some(n)
            if !n.trim().is_empty() && resolved.is_some_and(|p| p.role == Some(Role::Variable)) =>
        {
            n.as_str()
        }
        _ => resolved
            .map(|p| p.name)
            .unwrap_or_else(|| block.kind.as_str()),
    }
}

/// A block's caption (ADR-0024 §6): its kind's template with each
/// `{name}` hole filled — a wired port of that name shows the block at
/// the wire's other end (a boundary pseudo-block its port's name), else
/// the parameter's value or its default; a hole naming neither is `?`.
fn caption_for(
    block: &Block,
    resolved: Option<&BlockPorts>,
    blocks: &[Block],
    wires: &[Wire],
    flow: &Flow,
    libs: &[FlowLibrary],
) -> Option<String> {
    let template = resolved?.caption?;
    let params = resolved.map(|p| p.params).unwrap_or(&[]);
    let hole = |name: &str| -> String {
        let other = wires.iter().find_map(|w| {
            if w.from_block == block.id && w.from_port == name {
                Some(w.to_block.as_str())
            } else if w.to_block == block.id && w.to_port == name {
                Some(w.from_block.as_str())
            } else {
                None
            }
        });
        if let Some(id) = other {
            if let Some(port) = id.strip_prefix(BOUNDARY) {
                return port.to_string();
            }
            if let Some(b) = blocks.iter().find(|b| b.id == id) {
                return display_name(b, flow.ports_of(libs, b).as_ref()).to_string();
            }
        }
        block
            .params
            .get(name)
            .cloned()
            .or_else(|| {
                params
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.default.clone())
            })
            .unwrap_or_else(|| "?".into())
    };
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        match rest[open..].find('}') {
            Some(close) => {
                out.push_str(&hole(&rest[open + 1..open + close]));
                rest = &rest[open + close + 1..];
            }
            None => {
                rest = &rest[open..];
                break;
            }
        }
    }
    out.push_str(rest);
    Some(out)
}

/// A port's place once its block is mirrored (left to right, in its own
/// frame) and then turned clockwise by quarter turns (ADR-0024 §5).
/// Offsets run left to right along top and bottom, top to bottom along
/// the sides — so a turn maps top to right at the same offset, right to
/// bottom reversed, bottom to left, left to top reversed.
fn turned(side: &'static str, offset: f32, turn: u8, mirror: bool) -> (&'static str, f32) {
    let (mut side, mut offset) = (side, offset);
    if mirror {
        (side, offset) = match side {
            "left" => ("right", offset),
            "right" => ("left", offset),
            other => (other, 1.0 - offset),
        };
    }
    for _ in 0..turn % 4 {
        (side, offset) = match side {
            "top" => ("right", offset),
            "right" => ("bottom", 1.0 - offset),
            "bottom" => ("left", offset),
            _ => ("top", 1.0 - offset),
        };
    }
    (side, offset)
}

/// The library a document is written in: the first block — at the root,
/// else inside a subflow — whose kind names a known library. `None` for
/// an empty flow (the palette then starts on the first library).
fn document_library(flow: &Flow, libs: &[FlowLibrary]) -> Option<usize> {
    flow.blocks
        .iter()
        .chain(flow.subflows.iter().flat_map(|s| s.blocks.iter()))
        .find_map(|b| {
            let (lib, _) = b.kind.split_once('/')?;
            libs.iter().position(|l| l.id == lib)
        })
}

/// Where a port sits when no symbol anchors it (ADR-0024): inputs on
/// the left, outputs on the right — dataflow reads left to right. Pins
/// (ADR-0020) alternate left and right, so a two-pin part (a resistor, a
/// spring) lies in its circuit's line instead of hanging both pins off
/// its top. A factor's first channel is the variable it generates and
/// leaves on the right; its parameters come in from the left
/// (ADR-0022). A variable's one pin rides its top.
fn default_side(direction: &str, pin: usize, pins: usize, role: Option<Role>) -> &'static str {
    match (direction, role) {
        ("in", _) => "left",
        ("out", _) => "right",
        (_, Some(Role::Variable)) => "top",
        (_, Some(Role::Factor)) => {
            if pin == 0 {
                "right"
            } else {
                "left"
            }
        }
        _ if pins == 1 || pin.is_multiple_of(2) => "left",
        _ => "right",
    }
}

/// One block's appearance, for [`SceneBuilder::push`] — the shape the
/// root scope and an entered subflow share (ADR-0021 §4).
struct BlockOut<'a> {
    id: String,
    name: &'a str,
    kind: &'a str,
    color: &'static str,
    pos: [f32; 2],
    /// The block's quarter turns and mirror (ADR-0024 §5).
    turn: u8,
    mirror: bool,
    /// Its filled caption (ADR-0024 §6).
    caption: Option<String>,
    resolved: Option<BlockPorts<'a>>,
}

/// Builds a scope's scene — the root flow's or an entered subflow's;
/// the engine never learns which level it draws (ADR-0021 §4).
#[derive(Default)]
struct SceneBuilder<'a> {
    index: BTreeMap<String, usize>,
    nodes: Vec<OutNode<'a>>,
    ports: Vec<OutPort>,
}

impl<'a> SceneBuilder<'a> {
    /// One block: the node, and its ports. A kind's symbol (ADR-0024)
    /// sizes the block, draws it and anchors its pins; whatever it leaves
    /// open follows [`default_side`].
    fn push(&mut self, block: BlockOut<'a>) {
        let BlockOut {
            id,
            name,
            kind,
            color,
            pos,
            turn,
            mirror,
            caption,
            resolved,
        } = block;
        let i = self.nodes.len();
        self.index.insert(id.clone(), i);
        let symbol = resolved.as_ref().and_then(|p| p.symbol);
        // Every port with its direction and its place: the symbol's
        // anchor, or the default side (offset spread below).
        let mut ports: Vec<(&Port, &'static str, &'static str, Option<f32>)> = Vec::new();
        if let Some(s) = &resolved {
            let all = s
                .inputs
                .iter()
                .map(|p| (p, "in"))
                .chain(s.outputs.iter().map(|p| (p, "out")))
                .chain(s.connections.iter().map(|p| (p, "both")));
            let mut pin = 0;
            for (p, direction) in all {
                let anchor = symbol.and_then(|sym| sym.anchors.iter().find(|a| a.port == p.name));
                let (side, offset) = match anchor {
                    Some(a) => (a.side.as_str(), Some(a.offset)),
                    None => {
                        let side = default_side(direction, pin, s.connections.len(), s.role);
                        (side, None)
                    }
                };
                if direction == "both" {
                    pin += 1;
                }
                ports.push((p, direction, side, offset));
            }
        }
        let mut on: BTreeMap<&str, usize> = BTreeMap::new();
        for p in ports.iter().filter(|p| p.3.is_none()) {
            *on.entry(p.2).or_default() += 1;
        }
        let tallest = on.get("left").max(on.get("right")).copied().unwrap_or(0);
        // A symbol has its own size; a box is tall enough for its ports,
        // with a little room for the label. A factor graph's variable is
        // a circle, as on paper (ADR-0022): it has no behaviour, only the
        // one pin factors attach to.
        let variable = resolved
            .as_ref()
            .is_some_and(|p| p.role == Some(Role::Variable));
        let (shape, width, height) = if let Some(sym) = symbol {
            ("rect", sym.width / 2.0, sym.height / 2.0)
        } else if variable {
            ("circle", 14.0, 14.0)
        } else {
            (
                "rect",
                BLOCK_W.max(name.len() as f32 * 1.9),
                (10.0 + 7.0 * tallest as f32).max(22.0),
            )
        };
        // A turned block's box is the turned one (ADR-0024 §5).
        let (width, height) = if turn % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        };
        self.nodes.push(OutNode {
            id,
            label: name,
            kind,
            color,
            shape,
            width,
            height,
            pos,
            pinned: true, // the document (or the declaration) owns the positions
            symbol: symbol.map(|s| s.path.as_str()),
            turn: turn % 4,
            mirror,
            caption,
        });
        // Unanchored ports spread evenly along their side, in order.
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for (p, direction, side, offset) in ports {
            let offset = offset.unwrap_or_else(|| {
                let k = seen.entry(side).or_default();
                *k += 1;
                *k as f32 / (on[side] + 1) as f32
            });
            let (side, offset) = turned(side, offset, turn, mirror);
            self.ports.push(OutPort {
                node: i,
                name: p.name.clone(),
                side,
                offset,
                direction,
                color: port_color(&p.ty),
            });
        }
    }

    /// The wires: a wire between two pins is an equation, drawn without
    /// an arrowhead (ADR-0020). `scope` prefixes an inner issue's
    /// `about` ("amp/w1") — a wire of this scope is bad when an issue
    /// names it.
    fn edges(
        self,
        wires: &'a [Wire],
        is_pin: &dyn Fn(&str, &str) -> bool,
        issues: &[Issue],
        scope: &str,
    ) -> OutScene<'a> {
        let bad_wire = |id: &str| issues.iter().any(|x| x.about == format!("{scope}{id}"));
        let edges = wires
            .iter()
            // A wire naming a gone block parses (validate reports it) — it
            // draws nothing here instead of panicking on the index.
            .filter_map(|w| {
                let (Some(&a), Some(&b)) = (
                    self.index.get(w.from_block.as_str()),
                    self.index.get(w.to_block.as_str()),
                ) else {
                    return None;
                };
                let directed =
                    !(is_pin(&w.from_block, &w.from_port) && is_pin(&w.to_block, &w.to_port));
                let color = bad_wire(&w.id).then_some("#e86b6a");
                Some(OutEdge {
                    id: &w.id,
                    a,
                    b,
                    directed,
                    a_port: &w.from_port,
                    b_port: &w.to_port,
                    route: OutRoute::Ortho {},
                    color,
                })
            })
            .collect();
        OutScene {
            nodes: self.nodes,
            edges,
            ports: self.ports,
        }
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
    /// The engine's double-click on a node — entering a subsystem block
    /// opens its subflow (ADR-0021 §4). The engine says `dblclick`, not
    /// camelCase.
    #[serde(rename = "dblclick")]
    DblClick {
        id: String,
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
    // The palette's library: the reader's choice once made, else the one
    // the document already uses (a RxInfer flow opens on RxInfer, not
    // on whichever library registered first).
    let mut chosen_lib: Signal<Option<usize>> = use_signal(|| None);
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
    // The open scope's stack of subflow ids (ADR-0021 §4): the root at
    // `[]`, a subflow once entered. The scene, the mutators and the
    // palette all serve whichever scope is on top; the engine never
    // knows a level exists.
    let mut scope: Signal<Vec<String>> = use_signal(Vec::new);
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
                            // An empty-canvas click pops a scope open
                            // (ADR-0021 §4) — the way out beside Escape.
                            if nodes.is_empty() && !scope.read().is_empty() {
                                scope.with_mut(|sc| {
                                    sc.pop();
                                });
                                selected.set((Vec::new(), Vec::new()));
                                editor_pos.set(None);
                            } else {
                                editor_pos
                                    .set(screen.filter(|s| s.len() == 2).map(|s| (s[0], s[1])));
                                selected.set((nodes, edges))
                            }
                        }
                        Ok(FromJs::DblClick { id }) => {
                            // A double-click on a subsystem block enters
                            // its subflow (ADR-0021 §4): the scene swaps,
                            // the breadcrumb names the stack.
                            let open = scope.read().clone();
                            let enters = flow
                                .read()
                                .scope(&open)
                                .and_then(|sc| {
                                    sc.blocks.iter().find(|b| b.id == id && b.subflow.is_some())
                                })
                                .and_then(|b| b.subflow.clone());
                            if let Some(sub) = enters {
                                if flow.read().subflow(&sub).is_some() {
                                    scope.with_mut(|sc| sc.push(sub));
                                    selected.set((Vec::new(), Vec::new()));
                                    editor_pos.set(None);
                                    notice.set(None);
                                }
                            }
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
                            let open = scope.read().clone();
                            let before = flow.read().to_json();
                            let (ok, why) = wire(&mut flow, &open, &libs, &from, &to);
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
                            let open = scope.read().clone();
                            let before = flow.read().to_json();
                            flow.with_mut(|f| {
                                let _ = f.with_scope(&open, |blocks, _| {
                                    for b in blocks.iter_mut() {
                                        if let Some(p) = positions.get(&b.id) {
                                            b.x = p[0] as f64;
                                            b.y = p[1] as f64;
                                        }
                                    }
                                });
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
    let mut fitted_scope: Signal<Vec<String>> = use_signal(Vec::new);
    use_effect(move || {
        let open = scope();
        if loaded_for_scene() {
            let libs = ws.contrib.flow_libraries.read().clone();
            push_graph(&eval_for_scene, &flow, &open, &libs);
            // The open scope's blocks bound the frame — entering a
            // scope refits by construction (its frame differs).
            let bounds = flow
                .read()
                .scope(&open)
                .map(|sc| sc.blocks.iter())
                .into_iter()
                .flatten()
                .fold(None::<[f64; 4]>, |a, b| match a {
                    None => Some([b.x, b.y, b.x, b.y]),
                    Some([x0, y0, x1, y1]) => {
                        Some([x0.min(b.x), y0.min(b.y), x1.max(b.x), y1.max(b.y)])
                    }
                });
            let scope_changed = *fitted_scope.peek() != open;
            if scope_changed {
                fitted.set(None);
                fitted_scope.set(open.clone());
            }
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
            scope.set(Vec::new());
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
        let open = scope.read().clone();
        let n = flow
            .read()
            .scope(&open)
            .map(|sc| sc.blocks.len())
            .unwrap_or(0);
        let (col, row) = ((n % 3) as f64, (n / 3) as f64);
        let params: BTreeMap<String, String> = k
            .params
            .iter()
            .map(|p| (p.name.clone(), p.default.clone()))
            .collect();
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            let _ = f.with_scope(&open, |blocks, _| {
                blocks.push(Block {
                    id: format!(
                        "{}-{}",
                        k.id,
                        &uuid::Uuid::new_v4().simple().to_string()[..6]
                    ),
                    kind: format!("{}/{}", lib.id, k.id),
                    x: 30.0 + col * 165.0,
                    y: 30.0 + row * 170.0,
                    params: params.clone(),
                    subflow: None, // a palette block is a library block
                    mirror: false,
                    turn: 0,
                });
            });
        });
        push_undo(undo_stack, redo_stack, before);
        notice.set(None);
        sync(&flow, &mut last_text, &mut doc, &broken);
        // No scene push — the flow change re-runs the scene effect.
    };

    // The palette's Subsystem button creates a subflow and a block
    // referencing it (ADR-0021 stage B): an empty subflow you enter
    // (double-click the block) and fill. Declaring boundary ports is
    // its own UX, deliberately not decided by the ADR — today the
    // declaration is written in the file.
    let create_subflow = move |_| {
        if broken.peek().is_some() {
            return;
        }
        let open = scope.read().clone();
        let n = flow.read().subflows.len();
        let placed = flow
            .read()
            .scope(&open)
            .map(|sc| sc.blocks.len())
            .unwrap_or(0);
        let (col, row) = ((placed % 3) as f64, (placed / 3) as f64);
        let suffix = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            f.subflows.push(Subflow {
                id: format!("subflow-{suffix}"),
                name: format!("Subsystem {}", n + 1),
                ..Default::default()
            });
            let _ = f.with_scope(&open, |blocks, _| {
                blocks.push(Block {
                    id: format!("subsystem-{suffix}"),
                    kind: "subflow".into(),
                    x: 30.0 + col * 165.0,
                    y: 30.0 + row * 170.0,
                    params: Default::default(),
                    subflow: Some(format!("subflow-{suffix}")),
                    mirror: false,
                    turn: 0,
                });
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
        let open = scope.read().clone();
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            let _ = f.with_scope(&open, |blocks, _| {
                if let Some(b) = blocks.iter_mut().find(|b| b.id == bid) {
                    b.params.insert(name.clone(), value.clone());
                }
            });
        });
        push_undo(undo_stack, redo_stack, before);
        sync(&flow, &mut last_text, &mut doc, &broken);
    };

    // R turns the given blocks a quarter clockwise, M mirrors them left
    // to right as seen (ADR-0024 §5) — one undo step each.
    let mut reorient = move |(ids, mirror): (Vec<String>, bool)| {
        if broken.peek().is_some() || ids.is_empty() {
            return;
        }
        let open = scope.read().clone();
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            let _ = f.with_scope(&open, |blocks, _| {
                for b in blocks.iter_mut().filter(|b| ids.contains(&b.id)) {
                    if mirror {
                        b.flip();
                    } else {
                        b.rotate();
                    }
                }
            });
        });
        if flow.read().to_json() != before {
            push_undo(undo_stack, redo_stack, before);
            sync(&flow, &mut last_text, &mut doc, &broken);
        }
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
        let open = scope.read().clone();
        let before = flow.read().to_json();
        flow.with_mut(|f| {
            let _ = f.with_scope(&open, |blocks, scope_wires| {
                blocks.retain(|b| !nodes.contains(&b.id));
                scope_wires.retain(|w| {
                    !wires.contains(&w.id)
                        && !nodes.contains(&w.from_block)
                        && !nodes.contains(&w.to_block)
                });
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
        // A snapshot is the whole document; a scope the restored
        // document no longer holds (its subflow was deleted) pops off
        // the stack (ADR-0021 §4).
        let mut open = scope.read().clone();
        {
            let f = flow.read();
            while !open.is_empty() && f.scope(&open).is_none() {
                open.pop();
            }
        }
        scope.set(open);
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
        let mut open = scope.read().clone();
        {
            let f = flow.read();
            while !open.is_empty() && f.scope(&open).is_none() {
                open.pop();
            }
        }
        scope.set(open);
        selected.set((Vec::new(), Vec::new()));
        editor_pos.set(None);
        notice.set(None);
        sync(&flow, &mut last_text, &mut doc, &broken);
    };

    // The selected block's parameters (spec 031 §7: one editor for the
    // selected node, not a component per block).
    let (sel_blocks, _sel_wires) = selected.read().clone();
    let sel_block = sel_blocks.first().and_then(|id| {
        let open = scope.read().clone();
        flow.read()
            .scope(&open)
            .and_then(|sc| sc.blocks.iter().find(|b| b.id == *id).cloned())
    });
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
                } else if e.key() == Key::Escape {
                    // The way out of a subflow, beside the empty-canvas
                    // click (ADR-0021 §4).
                    if !scope.read().is_empty() {
                        scope.with_mut(|sc| {
                            sc.pop();
                        });
                        selected.set((Vec::new(), Vec::new()));
                        editor_pos.set(None);
                    }
                } else if matches!(e.key(), Key::Character(ref c) if c.eq_ignore_ascii_case("r") || c.eq_ignore_ascii_case("m"))
                    && !e.modifiers().alt()
                {
                    // R turns, M mirrors the selected blocks (ADR-0024 §5).
                    let mirror = matches!(e.key(), Key::Character(ref c) if c.eq_ignore_ascii_case("m"));
                    let ids = selected.read().0.clone();
                    if !ids.is_empty() {
                        e.prevent_default();
                        reorient((ids, mirror));
                    }
                } else if matches!(e.key(), Key::Delete | Key::Backspace) {
                    // The editor stops its own keys (below), so a key
                    // that reaches the root is aimed at the canvas.
                    e.prevent_default();
                    delete_selected(());
                }
            },
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path",
                    "{doc.read().node.native_key}",
                    // The breadcrumb of an open scope stack
                    // (ADR-0021 §4): parent / sub_1 / …
                    for sid in scope.read().iter() {
                        { if let Some(sub) = flow.read().subflow(sid) { format!(" / {}", sub.name) } else { String::new() } }
                    }
                }
                if dirty { span { class: "mk-editor-dirty", title: t!(ws, L, "flow-unsaved"), "●" } }
                span { class: "mk-editor-spacer" }
                if let Some(n) = notice() { span { class: "mk-flow-notice", "{n}" } }
                span { class: "mk-flow-status", "data-blocks": "{flow.read().scope(&scope.read()).map(|sc| sc.blocks.len()).unwrap_or(0)}", "data-wires": "{flow.read().scope(&scope.read()).map(|sc| sc.wires.len()).unwrap_or(0)}", "data-issues": "{issues_now.len()}",
                    {t!(ws, L, "flow-counts", blocks = flow.read().blocks.len(), wires = flow.read().wires.len())}
                    if !issues_now.is_empty() { span { class: "mk-flow-issues", title: "{issues_now.iter().map(|i| i.message.clone()).collect::<Vec<_>>().join(\"\\n\")}", {t!(ws, L, "flow-issues", n = issues_now.len())} } }
                }
                for lib in generators {
                    // The button names the library, not just the language —
                    // two Julia libraries must not read as one button.
                    button { key: "{lib.id}", class: "mk-btn mk-flow-generate", onclick: { let lib = lib.clone(); move |_| generate(lib.clone()) }, title: t!(ws, L, "flow-generate-title", language = lib.name), {t!(ws, L, "flow-generate", language = lib.name)} }
                }
                // While broken there is nothing to arrange, fit or save —
                // only Reload leads out (the review's #3: the toolbar
                // must not offer edits the model refuses to write).
                // Symbols, with their description on hover (ADR-0027);
                // Generate keeps its words, the panel's primary action.
                moonkale_ext_api::icons::IconButton {
                    icon: "layout",
                    label: t!(ws, L, "flow-layout"),
                    description: t!(ws, L, "flow-layout-title"),
                    disabled: broken.read().is_some(),
                    onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::HierLayout); } },
                }
                moonkale_ext_api::icons::IconButton {
                    icon: "fit",
                    label: t!(ws, L, "flow-fit"),
                    description: t!(ws, L, "flow-fit-title"),
                    disabled: broken.read().is_some(),
                    onclick: move |_| { if let Some(ev) = eval.peek().as_ref() { let _ = ev.send(ToJs::Fit); } },
                }
                moonkale_ext_api::icons::IconButton {
                    icon: "save",
                    label: t!(ws, L, "flow-save"),
                    shortcut: format!("{}+S", moonkale_ext_api::keys::primary_name()),
                    disabled: !dirty || broken.read().is_some(),
                    onclick: move |_| save(()),
                }
                moonkale_ext_api::icons::IconButton {
                    icon: "reload",
                    label: t!(ws, L, "flow-reload"),
                    description: t!(ws, L, "flow-reload-title"),
                    class: "mk-flow-reload".to_string(),
                    onclick: move |_| { spawn(async move { if let Err(e) = ws.reload(node).await { error.set(Some(e.to_string())); } }); },
                }
                moonkale_ext_api::icons::PanelSettingsButton {}
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
                            {
                                let current = chosen_lib()
                                    .filter(|i| *i < libs.len())
                                    .or_else(|| document_library(&flow.read(), &libs))
                                    .unwrap_or(0);
                                rsx! {
                                    if libs.len() > 1 {
                                        select { class: "mk-input", onchange: move |e| chosen_lib.set(e.value().parse().ok()),
                                            for (i, l) in libs.iter().enumerate() { option { value: "{i}", selected: i == current, "{l.name}" } }
                                        }
                                    }
                                }
                            }
                            {
                                let current = chosen_lib()
                                    .filter(|i| *i < libs.len())
                                    .or_else(|| document_library(&flow.read(), &libs))
                                    .unwrap_or(0);
                                let lib = libs.get(current).or(libs.first()).cloned();
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
                    // The built-in palette entry: a subsystem,
                    // whatever libraries are enabled (ADR-0021 B).
                    button {
                        class: "mk-flow-palette-block mk-flow-subsystem",
                        title: t!(ws, L, "flow-subsystem-title"),
                        onclick: {
                            let mut create_subflow = create_subflow;
                            move |_| create_subflow(())
                        },
                        {t!(ws, L, "flow-subsystem")}
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
                                div { class: "mk-flow-props-title",
                                    span { "{b.id}" }
                                    {
                                        let (r, m) = (b.id.clone(), b.id.clone());
                                        let (mut turn, mut flip) = (reorient, reorient);
                                        rsx! {
                                            button { class: "mk-btn mk-flow-rotate", title: t!(ws, L, "flow-rotate-title"),
                                                onclick: move |_| turn((vec![r.clone()], false)), {t!(ws, L, "flow-rotate")} }
                                            button { class: "mk-btn mk-flow-mirror", title: t!(ws, L, "flow-mirror-title"),
                                                onclick: move |_| flip((vec![m.clone()], true)), {t!(ws, L, "flow-mirror")} }
                                        }
                                    }
                                }
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

/// Re-send the scene after a mutation (the engine matches by id). The
/// open scope's scene — the root's or the entered subflow's.
fn push_graph(
    eval: &Signal<Option<Eval>>,
    flow: &Signal<Flow>,
    scope: &[String],
    libs: &[FlowLibrary],
) {
    if let Some(ev) = eval.peek().as_ref() {
        let f = flow.read();
        let issues = validate(&f, libs);
        let scene = match f.scope(scope).and_then(|sc| {
            scope
                .last()
                .and_then(|id| f.subflow(id))
                .map(|sub| (sub, sc))
        }) {
            // An entered subflow: its own blocks and wires, plus its
            // declared boundary as pseudo-blocks (ADR-0021 §3).
            Some((sub, _)) => subflow_scene(&f, sub, libs, &issues),
            None => to_scene(&f, libs, &issues),
        };
        let _ = ev.send(ToJs::SetGraph { scene });
    }
}

/// One wire per input port (a new one replaces the old), only when the
/// types unify — and a pin wire, an equation the ModelingToolkit rung
/// needs (ADR-0020), connects in either drag order, may share its pin
/// with other wires (a bus), and never replaces anything. The renderer
/// never mutates the model. The wire lands in the open scope
/// (ADR-0021 §4): at the root, or inside the entered subflow — where a
/// boundary pseudo-block resolves through the declaration.
fn wire(
    flow: &mut Signal<Flow>,
    path: &[String],
    libs: &[FlowLibrary],
    from: &End,
    to: &End,
) -> (bool, String) {
    /// A wire is dataflow (leaves an output, enters an input) or an
    /// equation (both ends are pins) — the retention rule follows.
    enum Accepted {
        Dataflow,
        Equation,
    }
    // The type check reads; the write mutates — the two must not overlap.
    let checked = {
        let f = flow.read();
        let Some(sc) = f.scope(path) else {
            return (false, "wire: unknown scope".into());
        };
        let ports_of = |id: &str| -> Option<BlockPorts<'_>> {
            match path.last().and_then(|sid| f.subflow(sid)) {
                // Inside a subflow, `inner_ports` resolves a boundary
                // pseudo-block through the declaration (ADR-0021 §3)
                // and every other block as at the root.
                Some(sub) => inner_ports(&f, sub, libs, id),
                None => f.block(id).and_then(|b| f.ports_of(libs, b)),
            }
        };
        let (Some(fp), Some(tp)) = (ports_of(&from.node), (ports_of(&to.node))) else {
            return (false, "wire: unknown block kind".into());
        };
        let out = fp.outputs.iter().find(|p| p.name == from.port);
        let inp = tp.inputs.iter().find(|p| p.name == to.port);
        let pin_f = fp.connections.iter().find(|p| p.name == from.port);
        let pin_t = tp.connections.iter().find(|p| p.name == to.port);
        if let (Some(o), Some(i)) = (out, inp) {
            match o.ty.unify(&i.ty) {
                Some(_) => Ok(Accepted::Dataflow),
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
        } else if let (Some(a), Some(b)) = (pin_f, pin_t) {
            // Either drag order; domains unify symmetrically.
            match a.ty.unify(&b.ty).or_else(|| b.ty.unify(&a.ty)) {
                Some(_) => {
                    // The same pin on both ends is a short, and an equation
                    // already stored in either order is the same equation —
                    // store neither twice (the pins review's #3). Two
                    // different pins of one block are a real short circuit.
                    if from.node == to.node && from.port == to.port {
                        return (false, "a pin does not connect to itself".into());
                    }
                    let same = sc.wires.iter().any(|w| {
                        (w.from_block == from.node
                            && w.from_port == from.port
                            && w.to_block == to.node
                            && w.to_port == to.port)
                            || (w.from_block == to.node
                                && w.from_port == to.port
                                && w.to_block == from.node
                                && w.to_port == from.port)
                    });
                    if same {
                        return (false, "the same equation is already wired".into());
                    }
                    // A factor graph is bipartite, and a factor's
                    // channel holds one variable (ADR-0022) — the same
                    // words `validate` uses.
                    if let Some(why) = bipartite_refusal(&fp, &tp) {
                        return (false, why);
                    }
                    let degree = |node: &str, port: &str| {
                        sc.wires
                            .iter()
                            .filter(|w| {
                                (w.from_block == node && w.from_port == port)
                                    || (w.to_block == node && w.to_port == port)
                            })
                            .count()
                    };
                    for (ports, end) in [(&fp, from), (&tp, to)] {
                        if let Some(why) = channel_refusal(
                            ports,
                            &end.node,
                            &end.port,
                            degree(&end.node, &end.port),
                        ) {
                            return (false, why);
                        }
                    }
                    Ok(Accepted::Equation)
                }
                None => Err(format!(
                    "{}.{} ({}) does not connect to {}.{} ({})",
                    from.node,
                    from.port,
                    a.ty.describe(),
                    to.node,
                    to.port,
                    b.ty.describe()
                )),
            }
        } else if out.is_some() || inp.is_some() || pin_f.is_some() || pin_t.is_some() {
            // The ports exist but cannot pair (an output against a pin).
            return (
                false,
                "an output connects to an input, a pin to a pin".into(),
            );
        } else {
            return (false, "wire: the port does not exist".into());
        }
    };
    match checked {
        Ok(kind) => {
            let mut taken = false;
            flow.with_mut(|f| {
                if f.with_scope(path, |_, wires| {
                    // One wire per input port — a dataflow rule. A pin is a
                    // bus: every wire to it is another `connect` argument.
                    if matches!(kind, Accepted::Dataflow) {
                        wires.retain(|w| !(w.to_block == to.node && w.to_port == to.port));
                    }
                    wires.push(Wire {
                        id: format!("w-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
                        from_block: from.node.clone(),
                        from_port: from.port.clone(),
                        to_block: to.node.clone(),
                        to_port: to.port.clone(),
                    });
                })
                .is_some()
                {
                    taken = true;
                }
            });
            (taken, String::new())
        }
        Err(why) => (false, why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_ext_api::flow::Subflow;
    use moonkale_ext_api::flow::{Anchor, Side, Symbol};

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

    // ---- ADR-0020: undirected pins -------------------------------------

    use moonkale_ext_api::flow::{BlockKind, FlowLibrary, Port, PortType};

    /// A mixed library: two acausal components (electrical pins) and one
    /// directed pair, so both wire rules are exercised against the panel.
    fn pin_lib() -> FlowLibrary {
        let pin = |n: &str| Port {
            name: n.into(),
            ty: PortType::Named {
                name: "electrical".into(),
            },
            required: false,
        };
        let port = |n: &str, ty: PortType, required: bool| Port {
            name: n.into(),
            ty,
            required,
        };
        FlowLibrary {
            id: "m",
            name: "MTK",
            language: "none",
            codegen: None,
            blocks: vec![
                BlockKind {
                    id: "resistor".into(),
                    name: "Resistor".into(),
                    category: "electrical".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![],
                    connections: vec![pin("p"), pin("n")],
                    params: vec![],
                    role: None,
                    symbol: None,
                    caption: None,
                },
                BlockKind {
                    id: "ground".into(),
                    name: "Ground".into(),
                    category: "electrical".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![],
                    connections: vec![pin("g")],
                    params: vec![],
                    role: None,
                    symbol: None,
                    caption: None,
                },
                BlockKind {
                    id: "src".into(),
                    name: "Source".into(),
                    category: "io".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![port("out", PortType::Scalar, false)],
                    connections: vec![],
                    params: vec![],
                    role: None,
                    symbol: None,
                    caption: None,
                },
                BlockKind {
                    id: "mass".into(),
                    name: "Mass".into(),
                    category: "mechanical".into(),
                    description: String::new(),
                    inputs: vec![],
                    outputs: vec![],
                    connections: vec![Port {
                        name: "flange".into(),
                        ty: PortType::Named {
                            name: "mechanical".into(),
                        },
                        required: false,
                    }],
                    params: vec![],
                    role: None,
                    symbol: None,
                    caption: None,
                },
                BlockKind {
                    id: "sink".into(),
                    name: "Sink".into(),
                    category: "io".into(),
                    description: String::new(),
                    inputs: vec![port("in", PortType::Scalar, false)],
                    outputs: vec![],
                    connections: vec![],
                    params: vec![],
                    role: None,
                    symbol: None,
                    caption: None,
                },
            ],
        }
    }

    /// A block referencing a subflow, for the scope tests.
    fn sub_block(id: &str, sub: &str) -> Block {
        Block {
            id: id.into(),
            kind: "subflow".into(),
            subflow: Some(sub.into()),
            ..Default::default()
        }
    }

    fn end(node: &str, port: &str) -> End {
        End {
            node: node.into(),
            port: port.into(),
        }
    }

    #[test]
    fn subflow_scene_draws_the_boundary_as_pseudo_blocks() {
        let libs = vec![pin_lib()];
        let mut f = Flow::new();
        let mut amp = sub_amp();
        // Inside: a resistor fed by the boundary input x, joined at the
        // boundary pin p.
        amp.blocks = vec![Block {
            id: "r".into(),
            kind: "m/resistor".into(),
            ..Default::default()
        }];
        amp.wires = vec![
            Wire {
                id: "w1".into(),
                from_block: "boundary/x".into(),
                from_port: "x".into(),
                to_block: "r".into(),
                to_port: "p".into(),
            },
            Wire {
                id: "w2".into(),
                from_block: "boundary/p".into(),
                from_port: "p".into(),
                to_block: "r".into(),
                to_port: "n".into(),
            },
        ];
        f.subflows = vec![amp];
        let sub = f.subflow("amp").unwrap();
        let scene = subflow_scene(&f, sub, &libs, &[]);
        // The declared boundary rides the edges: x on the left, y on the
        // right, p on the top (ADR-0021 §3).
        let by_id = |id: &str| scene.nodes.iter().find(|n| n.id == id);
        assert_eq!(by_id("boundary/x").unwrap().kind, "boundary/in");
        assert_eq!(by_id("boundary/y").unwrap().kind, "boundary/out");
        assert_eq!(by_id("boundary/p").unwrap().kind, "boundary/pin");
        assert!(scene.nodes.iter().any(|n| n.id == "r"));
        // w2 is an equation through the boundary pin — no arrow.
        assert!(!scene.edges.iter().find(|e| e.id == "w2").unwrap().directed);
        // The boundary pseudo-blocks carry their declared port: x is an
        // output inside (a source), y an input, p a pin.
        let x_out = scene
            .ports
            .iter()
            .any(|p| p.name == "x" && p.direction == "out");
        let y_in = scene
            .ports
            .iter()
            .any(|p| p.name == "y" && p.direction == "in");
        let p_pin = scene
            .ports
            .iter()
            .any(|p| p.name == "p" && p.direction == "both");
        assert!(x_out && y_in && p_pin, "the boundary's sides flip inside");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wires_land_in_the_open_scope_and_through_its_boundary() {
        #[component]
        fn Host() -> Element {
            rsx! {}
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let libs = vec![pin_lib()];
        let mut flow = dom.in_scope(ScopeId::ROOT, || {
            Signal::new_in_scope(Flow::new(), ScopeId::ROOT)
        });
        flow.with_mut(|f| {
            let mut amp = sub_amp();
            amp.blocks = vec![
                Block {
                    id: "r".into(),
                    kind: "m/resistor".into(),
                    ..Default::default()
                },
                Block {
                    id: "k".into(),
                    kind: "m/sink".into(),
                    ..Default::default()
                },
            ];
            f.subflows = vec![amp];
            f.blocks = vec![sub_block("sub_1", "amp")];
        });
        let path = vec!["amp".to_string()];
        // A dataflow wire from the boundary input x (a source inside)
        // into the sink.
        let (ok, _) = wire(
            &mut flow,
            &path,
            &libs,
            &end("boundary/x", "x"),
            &end("k", "in"),
        );
        assert!(ok, "the boundary input feeds the inside");
        // The boundary pin joins the inside — an equation, either order.
        let (ok, _) = wire(
            &mut flow,
            &path,
            &libs,
            &end("boundary/p", "p"),
            &end("r", "n"),
        );
        assert!(ok, "the boundary pin joins the inside");
        // The wires landed in the subflow, not the root.
        assert_eq!(flow.read().subflow("amp").unwrap().wires.len(), 2);
        assert!(flow.read().wires.is_empty());
        // A mechanical flange onto the boundary pin: refused inside too.
        flow.with_mut(|f| {
            f.subflows
                .iter_mut()
                .find(|s| s.id == "amp")
                .unwrap()
                .blocks
                .push(Block {
                    id: "mm".into(),
                    kind: "m/mass".into(),
                    ..Default::default()
                });
        });
        let (ok, why) = wire(
            &mut flow,
            &path,
            &libs,
            &end("mm", "flange"),
            &end("boundary/p", "p"),
        );
        assert!(!ok, "the domain rule holds through the boundary");
        assert!(why.contains("does not connect"), "{why}");
    }

    /// `wire` lives on signals; a parked VirtualDom gives the test a
    /// runtime scope for them (the desktop smoke check's trick, in-process).
    #[tokio::test(flavor = "current_thread")]
    async fn pin_wires_accept_both_orders_buses_and_keep_replacement_rules() {
        #[component]
        fn Host() -> Element {
            rsx! {}
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let libs = vec![pin_lib()];
        let mut flow = dom.in_scope(ScopeId::ROOT, || {
            Signal::new_in_scope(Flow::new(), ScopeId::ROOT)
        });
        flow.with_mut(|f| {
            f.blocks = vec![
                Block {
                    id: "r".into(),
                    kind: "m/resistor".into(),
                    ..Default::default()
                },
                Block {
                    id: "g".into(),
                    kind: "m/ground".into(),
                    ..Default::default()
                },
                Block {
                    id: "r2".into(),
                    kind: "m/resistor".into(),
                    ..Default::default()
                },
                Block {
                    id: "s".into(),
                    kind: "m/src".into(),
                    ..Default::default()
                },
                Block {
                    id: "k".into(),
                    kind: "m/sink".into(),
                    ..Default::default()
                },
            ];
        });

        // Either drag order connects (an equation has no direction).
        let (ok, _) = wire(&mut flow, &[], &libs, &end("r", "p"), &end("g", "g"));
        assert!(ok, "pin → pin connects");
        let (ok, _) = wire(&mut flow, &[], &libs, &end("g", "g"), &end("r2", "n"));
        assert!(ok, "the same pin accepts the other drag order");

        // A pin is a bus: the ground's one pin now carries two wires, both kept.
        assert_eq!(flow.read().wires.len(), 2);
        assert_eq!(
            flow.read()
                .wires
                .iter()
                .filter(|w| w.from_port == "g" || w.to_port == "g")
                .count(),
            2,
            "a pin may carry several wires"
        );

        // The same equation twice — either drag order — and a pin onto
        // itself: stored neither (the pins review's #3).
        let (ok, _) = wire(&mut flow, &[], &libs, &end("r", "p"), &end("g", "g"));
        assert!(!ok, "the same equation is not stored twice");
        let (ok, _) = wire(&mut flow, &[], &libs, &end("g", "g"), &end("r", "p"));
        assert!(!ok, "nor in the other order");
        let (ok, _) = wire(&mut flow, &[], &libs, &end("r", "p"), &end("r", "p"));
        assert!(!ok, "a pin does not connect to itself");
        // Two different pins of one block: a short circuit, allowed.
        let (ok, _) = wire(&mut flow, &[], &libs, &end("r", "p"), &end("r", "n"));
        assert!(ok, "two pins of the same block make a short circuit");
        assert_eq!(
            flow.read().wires.len(),
            3,
            "the duplicate wires were refused"
        );

        // Directed wires keep their one-wire-per-input replacement.
        wire(&mut flow, &[], &libs, &end("s", "out"), &end("k", "in"));
        let (ok, _) = wire(&mut flow, &[], &libs, &end("s", "out"), &end("k", "in"));
        assert!(ok);
        assert_eq!(
            flow.read()
                .wires
                .iter()
                .filter(|w| w.to_port == "in")
                .count(),
            1,
            "a new input wire replaces the old one"
        );

        // A directed port onto a pin is a mixed wire — the ports exist,
        // they just do not pair (the pins review's #5).
        let (ok, why) = wire(&mut flow, &[], &libs, &end("s", "out"), &end("r", "p"));
        assert!(!ok, "an output does not connect to a pin");
        assert!(why.contains("a pin to a pin"), "{why}");
    }

    #[test]
    fn to_scene_draws_pins_on_the_sides_and_equations_without_arrows() {
        let libs = vec![pin_lib()];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
            Block {
                id: "g".into(),
                kind: "m/ground".into(),
                ..Default::default()
            },
            Block {
                id: "s".into(),
                kind: "m/src".into(),
                ..Default::default()
            },
            Block {
                id: "k".into(),
                kind: "m/sink".into(),
                ..Default::default()
            },
        ];
        f.wires = vec![
            Wire {
                id: "w1".into(),
                from_block: "r".into(),
                from_port: "p".into(),
                to_block: "g".into(),
                to_port: "g".into(),
            },
            Wire {
                id: "w2".into(),
                from_block: "s".into(),
                from_port: "out".into(),
                to_block: "k".into(),
                to_port: "in".into(),
            },
        ];
        let scene = to_scene(&f, &libs, &[]);
        // The equation carries no arrow; the dataflow wire does.
        assert!(!scene.edges.iter().find(|e| e.id == "w1").unwrap().directed);
        assert!(scene.edges.iter().find(|e| e.id == "w2").unwrap().directed);
        // A two-pin part lies in its line — p left, n right — and a lone
        // pin takes the left (ADR-0024); none rides the top. Drawn `both`.
        let pins: Vec<&OutPort> = scene
            .ports
            .iter()
            .filter(|p| p.direction == "both")
            .collect();
        assert_eq!(pins.len(), 3, "2 + 1 + 0 + 0 resistor/ground pins");
        let side = |node: usize, name: &str| {
            pins.iter()
                .find(|p| p.node == node && p.name == name)
                .unwrap()
                .side
        };
        assert_eq!(side(0, "p"), "left");
        assert_eq!(side(0, "n"), "right");
        assert_eq!(side(1, "g"), "left");
        assert!(
            scene.nodes.iter().all(|n| n.symbol.is_none()),
            "no kind draws a symbol"
        );
    }

    #[test]
    fn a_turn_maps_each_side_clockwise_and_a_mirror_swaps_left_and_right() {
        // A point at offset t on each side, turned a quarter clockwise.
        assert_eq!(turned("top", 0.2, 1, false), ("right", 0.2));
        assert_eq!(turned("right", 0.2, 1, false), ("bottom", 0.8));
        assert_eq!(turned("bottom", 0.2, 1, false), ("left", 0.2));
        assert_eq!(turned("left", 0.2, 1, false), ("top", 0.8));
        // Four turns are none; two put left on the right, reversed.
        assert_eq!(turned("left", 0.3, 4, false), ("left", 0.3));
        let (side, offset) = turned("left", 0.25, 2, false);
        assert_eq!(side, "right");
        assert!((offset - 0.75).abs() < 1e-6);
        // The mirror comes first, in the block's own frame.
        assert_eq!(turned("left", 0.25, 0, true), ("right", 0.25));
        assert_eq!(turned("top", 0.25, 0, true), ("top", 0.75));
        assert_eq!(turned("left", 0.5, 1, true), ("bottom", 0.5));
    }

    #[test]
    fn a_turned_block_swaps_its_box_and_turns_its_pins() {
        let mut lib = pin_lib();
        let r = lib.blocks.iter_mut().find(|k| k.id == "resistor").unwrap();
        r.symbol = Some(Symbol {
            width: 80.0,
            height: 32.0,
            path: "M 0 16 H 80".into(),
            anchors: vec![
                Anchor {
                    port: "p".into(),
                    side: Side::Left,
                    offset: 0.5,
                },
                Anchor {
                    port: "n".into(),
                    side: Side::Right,
                    offset: 0.5,
                },
            ],
        });
        let libs = vec![lib];
        let mut f = Flow::new();
        f.blocks = vec![Block {
            id: "r".into(),
            kind: "m/resistor".into(),
            turn: 1,
            ..Default::default()
        }];
        let scene = to_scene(&f, &libs, &[]);
        let n = &scene.nodes[0];
        assert_eq!(
            (n.width, n.height, n.turn),
            (16.0, 40.0, 1),
            "standing upright"
        );
        let side = |name: &str| scene.ports.iter().find(|p| p.name == name).unwrap().side;
        assert_eq!((side("p"), side("n")), ("top", "bottom"));
        // Mirrored left to right as seen, an upright part keeps its ends
        // (the drawing flips across its own axis).
        f.blocks[0].flip();
        let scene = to_scene(&f, &libs, &[]);
        let side = |name: &str| scene.ports.iter().find(|p| p.name == name).unwrap().side;
        assert_eq!((side("p"), side("n")), ("top", "bottom"));
        assert_eq!((scene.nodes[0].turn, scene.nodes[0].mirror), (3, true));
        // Turned on to lie down again, mirrored: p on the right.
        f.blocks[0].rotate();
        let scene = to_scene(&f, &libs, &[]);
        let side = |name: &str| scene.ports.iter().find(|p| p.name == name).unwrap().side;
        assert_eq!((side("p"), side("n")), ("right", "left"));
    }

    #[test]
    fn a_caption_fills_its_holes_from_wires_then_params_then_defaults() {
        let mut lib = rx_lib();
        let k = lib.blocks.iter_mut().find(|k| k.id == "normal").unwrap();
        k.caption = Some("{out} ~ N({mean}, {variance}) {s}".into());
        k.params = vec![moonkale_ext_api::flow::Param {
            name: "variance".into(),
            kind: ParamKind::Float,
            default: "1.0".into(),
        }];
        let libs = vec![lib];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "f".into(),
                kind: "rx/normal".into(),
                params: [("s".to_string(), "2".to_string())].into(),
                ..Default::default()
            },
            Block {
                id: "t".into(),
                kind: "rx/variable".into(),
                params: [("name".to_string(), "θ".to_string())].into(),
                ..Default::default()
            },
        ];
        let caption = |f: &Flow| {
            to_scene(f, &libs, &[])
                .nodes
                .iter()
                .find(|n| n.id == "f")
                .unwrap()
                .caption
                .clone()
        };
        // Unwired: a hole naming nothing is `?`, a parameter its value or
        // its default.
        assert_eq!(caption(&f).as_deref(), Some("? ~ N(?, 1.0) 2"));
        // A wired port names the block at the other end.
        f.wires = vec![Wire {
            id: "w".into(),
            from_block: "t".into(),
            from_port: "x".into(),
            to_block: "f".into(),
            to_port: "out".into(),
        }];
        assert_eq!(caption(&f).as_deref(), Some("θ ~ N(?, 1.0) 2"));
        let scene = to_scene(&f, &libs, &[]);
        let t = scene.nodes.iter().find(|n| n.id == "t").unwrap();
        assert!(t.caption.is_none(), "a kind without a template has none");
    }

    #[test]
    fn a_symbol_sizes_the_block_draws_it_and_anchors_its_pins() {
        let mut lib = pin_lib();
        let k = lib.blocks.iter_mut().find(|k| k.id == "ground").unwrap();
        k.symbol = Some(Symbol {
            width: 30.0,
            height: 24.0,
            path: "M 15 0 V 10 M 3 10 H 27".into(),
            anchors: vec![Anchor {
                port: "g".into(),
                side: Side::Top,
                offset: 0.5,
            }],
        });
        let r = lib.blocks.iter_mut().find(|k| k.id == "resistor").unwrap();
        // Anchors only one pin: the other keeps its default side.
        r.symbol = Some(Symbol {
            width: 60.0,
            height: 20.0,
            path: "M 0 10 H 60".into(),
            anchors: vec![Anchor {
                port: "p".into(),
                side: Side::Bottom,
                offset: 0.25,
            }],
        });
        let libs = vec![lib];
        let mut f = Flow::new();
        f.blocks = vec![
            Block {
                id: "g".into(),
                kind: "m/ground".into(),
                ..Default::default()
            },
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
        ];
        let scene = to_scene(&f, &libs, &[]);
        let g = &scene.nodes[0];
        assert_eq!(g.symbol, Some("M 15 0 V 10 M 3 10 H 27"));
        assert_eq!(
            (g.shape, g.width, g.height),
            ("rect", 15.0, 12.0),
            "half-extents"
        );
        let port = |node: usize, name: &str| {
            scene
                .ports
                .iter()
                .find(|p| p.node == node && p.name == name)
                .unwrap()
        };
        assert_eq!(port(0, "g").side, "top");
        assert_eq!(port(1, "p").side, "bottom");
        assert_eq!(port(1, "p").offset, 0.25);
        assert_eq!(port(1, "n").side, "right", "unanchored: the default rule");
        assert_eq!(port(1, "n").offset, 0.5);
    }

    // ---- ADR-0021: subflows ---------------------------------------------

    /// A subflow with a declared boundary: one input, one output and an
    /// electrical pin.
    fn sub_amp() -> Subflow {
        let port = |n: &str, ty: PortType, required: bool| Port {
            name: n.into(),
            ty,
            required,
        };
        Subflow {
            id: "amp".into(),
            name: "Amplifier".into(),
            inputs: vec![port("x", PortType::Any, false)],
            outputs: vec![port("y", PortType::Any, false)],
            connections: vec![port(
                "p",
                PortType::Named {
                    name: "electrical".into(),
                },
                false,
            )],
            ..Default::default()
        }
    }

    #[test]
    fn to_scene_draws_a_subflow_block_with_its_declared_boundary() {
        let libs = vec![pin_lib()];
        let mut f = Flow::new();
        f.subflows = vec![sub_amp()];
        f.blocks = vec![
            Block {
                id: "sub_1".into(),
                kind: "subflow".into(),
                subflow: Some("amp".into()),
                ..Default::default()
            },
            Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            },
        ];
        // An equation onto the subsystem's declared pin, drawn like any
        // pin wire — without an arrow, from the block's left edge.
        f.wires = vec![Wire {
            id: "w1".into(),
            from_block: "sub_1".into(),
            from_port: "p".into(),
            to_block: "r".into(),
            to_port: "p".into(),
        }];
        let scene = to_scene(&f, &libs, &[]);
        let node = scene.nodes.iter().find(|n| n.id == "sub_1").unwrap();
        assert_eq!(
            node.label, "Amplifier",
            "the block shows the subflow's name"
        );
        assert_eq!(node.kind, "subflow");
        assert_eq!(node.color, "#b48cf0");
        assert!(
            !scene.edges[0].directed,
            "a boundary pin wire is an equation"
        );
        // The declared boundary is the parent block's ports: the input on
        // the left, the output on the right, the lone pin on the left too.
        let port = |name: &str| scene.ports.iter().find(|p| p.name == name && p.node == 0);
        assert_eq!(port("x").unwrap().side, "left");
        assert_eq!(port("x").unwrap().direction, "in");
        assert_eq!(port("y").unwrap().side, "right");
        assert_eq!(port("y").unwrap().direction, "out");
        assert_eq!(port("p").unwrap().side, "left");
        assert_eq!(port("p").unwrap().direction, "both");
    }

    /// `wire` lives on signals; a parked VirtualDom gives the test a
    /// runtime scope for them.
    #[tokio::test(flavor = "current_thread")]
    async fn wires_reach_a_subflow_block_through_its_boundary() {
        #[component]
        fn Host() -> Element {
            rsx! {}
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let libs = vec![pin_lib()];
        let mut flow = dom.in_scope(ScopeId::ROOT, || {
            Signal::new_in_scope(Flow::new(), ScopeId::ROOT)
        });
        flow.with_mut(|f| {
            f.subflows = vec![sub_amp()];
            f.blocks = vec![
                Block {
                    id: "sub_1".into(),
                    kind: "subflow".into(),
                    subflow: Some("amp".into()),
                    ..Default::default()
                },
                Block {
                    id: "r".into(),
                    kind: "m/resistor".into(),
                    ..Default::default()
                },
            ];
        });
        // The declared pin is a pin: an equation, either drag order.
        let (ok, _) = wire(&mut flow, &[], &libs, &end("r", "p"), &end("sub_1", "p"));
        assert!(ok, "a resistor's pin connects to the boundary pin");
        // The declared input is an input: one wire per input port, the
        // boundary included (a second source replaces the first).
        flow.with_mut(|f| {
            f.blocks.push(Block {
                id: "s".into(),
                kind: "m/src".into(),
                ..Default::default()
            });
            f.blocks.push(Block {
                id: "s2".into(),
                kind: "m/src".into(),
                ..Default::default()
            });
        });
        let (ok, _) = wire(&mut flow, &[], &libs, &end("s", "out"), &end("sub_1", "x"));
        assert!(ok, "the declared input takes a dataflow wire");
        let (ok, _) = wire(&mut flow, &[], &libs, &end("s2", "out"), &end("sub_1", "x"));
        assert!(ok);
        assert_eq!(
            flow.read()
                .wires
                .iter()
                .filter(|w| w.to_port == "x")
                .count(),
            1,
            "one wire per input, the boundary included"
        );
        // A missing subflow is refused, not panicked on.
        flow.with_mut(|f| f.subflows.clear());
        let (ok, why) = wire(&mut flow, &[], &libs, &end("r", "p"), &end("sub_1", "p"));
        assert!(!ok, "a block whose subflow is gone takes no wire");
        assert!(why.contains("unknown block kind"), "{why}");
    }

    /// A factor-graph library (ADR-0022): a variable, a factor with two
    /// channels.
    fn rx_lib() -> FlowLibrary {
        let pin = |n: &str| Port {
            name: n.into(),
            ty: PortType::Any,
            required: false,
        };
        FlowLibrary {
            id: "rx",
            name: "RxInfer",
            language: "none",
            codegen: None,
            blocks: vec![
                BlockKind {
                    id: "variable".into(),
                    name: "x".into(),
                    connections: vec![pin("x")],
                    role: Some(Role::Variable),
                    ..Default::default()
                },
                BlockKind {
                    id: "normal".into(),
                    name: "Normal".into(),
                    connections: vec![pin("out"), pin("mean")],
                    role: Some(Role::Factor),
                    ..Default::default()
                },
            ],
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_wire_gesture_keeps_a_factor_graph_bipartite() {
        #[component]
        fn Host() -> Element {
            rsx! {}
        }
        let mut dom = VirtualDom::new(Host);
        dom.rebuild_in_place();
        let libs = vec![rx_lib()];
        let mut flow = dom.in_scope(ScopeId::ROOT, || {
            Signal::new_in_scope(Flow::new(), ScopeId::ROOT)
        });
        flow.with_mut(|f| {
            for (id, kind) in [
                ("x", "variable"),
                ("y", "variable"),
                ("f", "normal"),
                ("g", "normal"),
            ] {
                f.blocks.push(Block {
                    id: id.into(),
                    kind: format!("rx/{kind}"),
                    ..Default::default()
                });
            }
        });
        let (ok, why) = wire(&mut flow, &[], &libs, &end("f", "out"), &end("g", "mean"));
        assert!(!ok, "factor – factor is refused");
        assert!(why.contains("factors connect to variables"), "{why}");
        let (ok, why) = wire(&mut flow, &[], &libs, &end("x", "x"), &end("y", "x"));
        assert!(!ok, "variable – variable is refused");
        assert!(why.contains("variables connect to factors"), "{why}");
        // Variable – factor, either drag order; a variable fans out.
        assert!(wire(&mut flow, &[], &libs, &end("f", "out"), &end("x", "x")).0);
        assert!(wire(&mut flow, &[], &libs, &end("x", "x"), &end("g", "mean")).0);
        // A channel holds one variable: f.out already has x.
        let (ok, why) = wire(&mut flow, &[], &libs, &end("y", "x"), &end("f", "out"));
        assert!(!ok, "a second variable on one channel is refused");
        assert!(why.contains("not a bus"), "{why}");
        assert_eq!(flow.read().wires.len(), 2);
        assert_eq!(validate(&flow.read(), &libs), vec![]);
        // Variables draw as circles, factors as blocks.
        flow.with_mut(|f| {
            f.blocks[0].params.insert("name".into(), "θ".into());
        });
        let f = flow.read();
        let scene = to_scene(&f, &libs, &[]);
        let shape = |id: &str| scene.nodes.iter().find(|n| n.id == id).unwrap().shape;
        assert_eq!(shape("x"), "circle");
        // A variable is labelled with its name once it has one.
        assert_eq!(scene.nodes.iter().find(|n| n.id == "x").unwrap().label, "θ");
        assert_eq!(
            scene.nodes.iter().find(|n| n.id == "f").unwrap().label,
            "Normal"
        );
        assert_eq!(shape("f"), "rect");
    }

    #[test]
    fn the_palette_opens_on_the_library_the_document_uses() {
        let libs = vec![pin_lib(), rx_lib()];
        let mut f = Flow::new();
        assert_eq!(document_library(&f, &libs), None, "an empty flow has none");
        f.blocks.push(Block {
            id: "x".into(),
            kind: "rx/variable".into(),
            ..Default::default()
        });
        assert_eq!(document_library(&f, &libs), Some(1));
        // A subflow block has no library; its inside decides.
        f.blocks = vec![Block {
            id: "s1".into(),
            kind: "subflow".into(),
            subflow: Some("s".into()),
            ..Default::default()
        }];
        f.subflows = vec![Subflow {
            id: "s".into(),
            blocks: vec![Block {
                id: "r".into(),
                kind: "m/resistor".into(),
                ..Default::default()
            }],
            ..Default::default()
        }];
        assert_eq!(document_library(&f, &libs), Some(0));
    }
}
