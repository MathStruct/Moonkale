//! Browser glue: the `GraphView` object the host talks to, the animation
//! loop, pointer handling, and the 2D-canvas label overlay.
//!
//! JS API (via wasm-bindgen, `--target web`):
//! ```js
//! const view = await create(canvas, overlay, (event) => { ... });
//! view.set_graph(JSON.stringify({nodes, edges}));  view.fit();  view.relayout();
//! view.resize(width, height, devicePixelRatio);     view.backend();  view.destroy();
//! ```
//! Events: `{kind:"ready", backend}`, `{kind:"hover", id, label, kind, key, x, y}` /
//! `{kind:"hover", id:null}`, `{kind:"click", id}`, `{kind:"dblclick", id}`,
//! `{kind:"settled"}`.

use crate::camera::Camera;
use crate::frame;
use crate::graph::{Graph, InDelta, InGraph};
use crate::layout::Layout;
use crate::render::Renderer;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

struct State {
    graph: Graph,
    egraph: Option<crate::egraph::EGraph>,
    rewrites: frame::Selection,
    layout: Layout,
    camera: Camera,
    reported_camera: Option<[f32; 6]>,
    space_pan: bool,
    renderer: Renderer,
    overlay: web_sys::CanvasRenderingContext2d,
    dpr: f32,
    hovered: Option<usize>,
    dragging: Drag,
    on_event: js_sys::Function,
    dirty: bool,
    alive: bool,
    /// Any edge carries a pulse (spec 031 §2, stage 6): the view redraws
    /// every frame while true — the clock moves even when nothing is
    /// dirty. Recomputed wherever the graph changes.
    animates: bool,
    /// Any node carries an icon (spec 031 §2, stage 8) or a symbol
    /// (ADR-0024): gates the overlay's ink scan — a second pass over every node per frame
    /// that a 100 k-node graph must not pay for nothing.
    has_icons: bool,
    /// The pointer's canvas position — the edge label rides it.
    pointer: (f32, f32),
    /// The edge under the pointer (spec 031 §2, stage 9) — its label
    /// names the stroke while hovered; a node hover wins over it.
    hovered_edge: Option<usize>,
    /// Any edge carries a label: gates the per-move edge scan, one
    /// pass over every drawn segment.
    has_edge_labels: bool,
    /// The keyboard focus node's id (spec 031, stage 10): Tab and the
    /// arrows walk the map, Enter clicks — a canvas graph is invisible
    /// to keyboards without it. Id-keyed, so it survives graph swaps
    /// like the selection.
    focus_id: Option<String>,
    /// The pulse clock's zero (`performance.now` at create); `camera.time`
    /// is the elapsed seconds since.
    t0_ms: f64,
    last_click_ms: f64,
    /// Fit the view when the layout settles — only until the user has moved
    /// the camera or a node; after that, their view is theirs.
    auto_fit: bool,
    /// Active touch pointers `(id, x, y)`, for two-finger gestures.
    touches: Vec<(i32, f32, f32)>,
    /// Views put aside by `stash(name)`: the graph with its positions and
    /// pins, the layout's state, the camera (Prompt26: Local must not
    /// disturb Whole).
    stashed: std::collections::HashMap<String, Stashed>,
    /// Label colours (spec 030: `--mk-graph-label`, `--mk-graph-label-hover`).
    label: String,
    label_hover: String,
    /// Instance-data revisions (spec 031, stage 0): positions move with
    /// every layout step and drag; styles on a graph swap, a layer toggle
    /// or a wire start/end; hover recolours nodes only; selection rings
    /// nodes and brightens edges. Each half uploads when its revision
    /// moves — hover no longer rebuilds segment styles (the stage 0–3
    /// review's perf note).
    pos_rev: u64,
    attr_rev: u64,
    hover_rev: u64,
    sel_rev: u64,
    /// The selection, by instance id (spec 031 §5) — ids survive an
    /// incremental `set_graph`; a fresh graph's stale ids match nothing.
    selected: frame::Selection,
    /// Whether the current drag moved anything: a click after a drag
    /// selects nothing (the pointer released a pan or a node move, not a
    /// pick).
    drag_moved: bool,
    /// Explore or edit (spec 031 §5); `set_mode` stays the 2D/3D switch.
    interaction: Interaction,
    /// The wire being dragged: its source port (node id, port name) and
    /// the current endpoint (edit mode; the pending wire, not yet an edge).
    wire_from: Option<(String, String)>,
    wire_to: Option<[f32; 3]>,
    /// Where the current press started: a release within ~4 px is a click,
    /// not a drag — a touch tap jitters 1–2 px (the stage 0–3 review).
    press: (f32, f32),
    /// The last `connect` event, as JSON, for tests.
    last_connect: Option<String>,
    // Last load: parse, build, install, first layout, first draw, load-to-draw (ms).
    load_stats: [f64; 6],
    load_started: f64,
    awaiting_first_draw: bool,
}

impl State {
    /// The id of the node being dragged, if any.
    fn dragged_id(&self) -> Option<String> {
        match self.dragging {
            Drag::Node { index } => self.graph.nodes.get(index).map(|n| n.id.clone()),
            _ => None,
        }
    }

    /// After the graph was replaced: a node drag follows its node to its new
    /// index, or ends when the node is gone (#12 — a stale index panicked
    /// every pointer move).
    fn keep_drag(&mut self, id: Option<String>) {
        if let Drag::Node { .. } = self.dragging {
            self.dragging = id
                .and_then(|id| self.graph.nodes.iter().position(|n| n.id == id))
                .map_or(Drag::None, |index| Drag::Node { index });
        }
    }
}

struct Stashed {
    graph: Graph,
    layout: Layout,
    camera: Camera,
    auto_fit: bool,
}

#[derive(Clone, PartialEq)]
enum Drag {
    None,
    Pan {
        last: (f32, f32),
    },
    /// Two fingers (spec 006): pinch zooms at the midpoint, the midpoint's
    /// motion pans, and the fingers' rotation orbits in 3D.
    Pinch {
        last_dist: f32,
        last_angle: f32,
        last_mid: (f32, f32),
    },
    Node {
        index: usize,
    },
    /// 3D: right button or Shift-drag turns the camera around its target.
    Orbit {
        last: (f32, f32),
    },
    /// Edit mode: dragging a wire out of a port (spec 031 §5, stage 2),
    /// held by node id + port name — the graph may be swapped under the
    /// gesture (P-155's lesson, again).
    Wire {
        node: String,
        name: String,
    },
}

/// The two interaction modes over one renderer (spec 031 §5): graph
/// exploration, and flow/canvas editing — ports, wire gestures.
#[derive(Clone, Copy, PartialEq)]
enum Interaction {
    Explore,
    Edit,
}

#[wasm_bindgen]
pub struct GraphView {
    state: Rc<RefCell<State>>,
}

/// The selection changed (spec 031 §5): the ids, sorted for a stable
/// payload.
fn emit_select(s: &mut State) {
    s.rewrites = frame::Selection::default();
    if let (Some(egraph), Some(id)) = (&s.egraph, s.selected.nodes.iter().next()) {
        let (nodes, edges) = egraph.ancestry(id);
        s.rewrites = frame::Selection { nodes, edges };
        s.sel_rev += 1;
        s.dirty = true;
    }
    let mut nodes: Vec<&String> = s.selected.nodes.iter().collect();
    nodes.sort();
    let mut edges: Vec<&String> = s.selected.edges.iter().collect();
    edges.sort();
    // The first selected node's screen position, so a host can open its
    // editor beside the block (spec 031 §7). Computed here, under the
    // borrow the event is emitted with — a host cannot call back into
    // the view from inside an event (the RefCell is held).
    let screen = nodes.first().and_then(|id| {
        let n = s.graph.nodes.iter().find(|n| &n.id == *id)?;
        if s.camera.three_d {
            s.camera.project(n.x, n.y, n.z).map(|(x, y, _)| vec![x, y])
        } else {
            let (x, y) = s.camera.world_to_screen(n.x, n.y);
            Some(vec![x, y])
        }
    });
    emit(
        s,
        serde_json::json!({ "kind": "select", "nodes": nodes, "edges": edges, "screen": screen }),
    );
}

/// A node with layers draws (and answers hits) only while one of them is
/// visible (spec 031 §3).
/// Resolve a (node id, port name) to its port index; `None` when the
/// graph no longer has it — a swap ends the gesture instead of panicking.
fn port_index(graph: &Graph, node: &str, name: &str) -> Option<usize> {
    let n = graph.nodes.iter().position(|n| n.id == node)?;
    graph
        .ports
        .iter()
        .position(|p| p.node == n && p.name == name)
}

/// End the pending wire, if any — its segment and its style leave the
/// buffers together (B4 of the stage 0–3 review).
fn end_wire(s: &mut State) {
    if s.wire_from.take().is_some() {
        s.wire_to = None;
        s.pos_rev += 1;
        s.attr_rev += 1;
        s.dirty = true;
    }
}

fn node_visible(graph: &Graph, i: usize) -> bool {
    graph.visible(i)
}

fn emit(state: &State, value: serde_json::Value) {
    let js = js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL);
    let _ = state.on_event.call1(&JsValue::NULL, &js);
}

/// Draw-time instrumentation (spec 031, stage 0): total milliseconds spent
/// in `Renderer::draw` and frames drawn, since module load. Read before and
/// after a workload to isolate the draw cost (`bench-draw.mjs`).
static DRAW_MICROS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static DRAW_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `[draw_ms_total, frames_drawn]` — deltas are the measurement.
#[wasm_bindgen]
pub fn draw_stats() -> Vec<f64> {
    use std::sync::atomic::Ordering;
    vec![
        DRAW_MICROS.load(Ordering::Relaxed) as f64 / 1000.0,
        DRAW_FRAMES.load(Ordering::Relaxed) as f64,
    ]
}

/// Layout-only benchmark for the wasm build (no GPU needed): milliseconds
/// per step on a synthetic graph of `n` nodes. Used by the Milestone 6
/// measurements (`packages/web/tests/e2e/bench.mjs`).
#[wasm_bindgen]
pub fn bench_layout(n: usize, steps: usize) -> f64 {
    use crate::graph::{InEdge, InGraph, InNode};
    let nodes = (0..n)
        .map(|i| InNode {
            id: i.to_string(),
            label: format!("n{i}"),
            kind: "file".into(),
            key: String::new(),
            color: None,
            layers: Vec::new(),
            ..Default::default()
        })
        .collect();
    let mut edges = Vec::new();
    for i in 1..n {
        edges.push(InEdge {
            a: i,
            b: i / 7,
            kind: "contains".into(),
            color: None,
            ..Default::default()
        });
        if i % 5 == 0 {
            edges.push(InEdge {
                a: i,
                b: (i * 7919) % n,
                kind: "links".into(),
                color: None,
                ..Default::default()
            });
        }
    }
    let mut g = crate::graph::Graph::from_input(InGraph {
        nodes,
        edges,
        ..Default::default()
    });
    let mut layout = crate::layout::Layout::new(&g);
    let perf = web_sys::window().and_then(|w| w.performance());
    let t0 = perf.as_ref().map(|p| p.now()).unwrap_or(0.0);
    for _ in 0..steps.max(1) {
        layout.step(&mut g);
    }
    let t1 = perf.as_ref().map(|p| p.now()).unwrap_or(0.0);
    (t1 - t0) / steps.max(1) as f64
}

#[wasm_bindgen]
pub async fn create(
    canvas: web_sys::HtmlCanvasElement,
    overlay: web_sys::HtmlCanvasElement,
    on_event: js_sys::Function,
    prefer: Option<String>,
) -> Result<GraphView, JsValue> {
    console_error_panic_hook::set_once();
    let dpr = web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0) as f32;
    let rect = canvas.get_bounding_client_rect();
    let (w, h) = (rect.width().max(1.0) as f32, rect.height().max(1.0) as f32);
    canvas.set_width((w * dpr) as u32);
    canvas.set_height((h * dpr) as u32);
    overlay.set_width((w * dpr) as u32);
    overlay.set_height((h * dpr) as u32);
    // `prefer = "gl"`: skip WebGPU (the Android WebView advertises it but
    // never finishes creating a device — P-089).
    let gl_only = prefer.as_deref() == Some("gl");
    let renderer = Renderer::new(canvas.clone(), (w * dpr) as u32, (h * dpr) as u32, gl_only)
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let ctx = overlay
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok())
        .ok_or_else(|| JsValue::from_str("no 2d context for the label overlay"))?;
    let graph = Graph::default();
    let has_icons = false;
    let has_edge_labels = false;
    let layout = Layout::new(&graph);
    let animates_at_create = animates(&graph);
    let state = Rc::new(RefCell::new(State {
        egraph: None,
        rewrites: frame::Selection::default(),
        camera: Camera {
            width: w,
            height: h,
            ..Default::default()
        },
        reported_camera: None,
        space_pan: false,
        graph,
        layout,
        renderer,
        overlay: ctx,
        dpr,
        hovered: None,
        dragging: Drag::None,
        on_event,
        dirty: true,
        alive: true,
        animates: animates_at_create,
        has_icons,
        pointer: (0.0, 0.0),
        hovered_edge: None,
        has_edge_labels,
        focus_id: None,
        t0_ms: web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0),
        last_click_ms: 0.0,
        auto_fit: true,
        touches: Vec::new(),
        stashed: std::collections::HashMap::new(),
        label: "rgba(230,232,238,0.85)".into(),
        label_hover: "#ffffff".into(),
        pos_rev: 1,
        attr_rev: 1,
        hover_rev: 1,
        sel_rev: 1,
        selected: frame::Selection::default(),
        drag_moved: false,
        interaction: Interaction::Explore,
        wire_from: None,
        wire_to: None,
        press: (0.0, 0.0),
        last_connect: None,
        load_stats: [0.0; 6],
        load_started: 0.0,
        awaiting_first_draw: false,
    }));
    let backend = state.borrow().renderer.backend.clone();
    emit(
        &state.borrow(),
        serde_json::json!({ "kind": "ready", "backend": backend }),
    );
    install_pointer_handlers(&overlay, state.clone());
    install_keyboard(&overlay, state.clone());
    start_loop(state.clone());
    Ok(GraphView { state })
}

/// The overlay becomes keyboard-focusable (spec 031, stage 10): the
/// canvas takes a tabindex — its pointerdown prevents the default, which
/// would otherwise stop the browser's focus-on-mousedown, so a
/// capture-phase listener focuses it first — and the arrow keys walk the
/// map spatially, Enter clicks the focused node and Escape clears the
/// selection. The walk emits `focus { id }` and wears a ring on the
/// label overlay, so hosts and keyboard users can both see it. Tab
/// stays the browser's: a canvas widget must not trap it.
fn install_keyboard(overlay: &web_sys::HtmlCanvasElement, state: Rc<RefCell<State>>) {
    let _ = overlay.set_attribute("tabindex", "0");
    {
        let el = overlay.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |_| {
            let _ = el.focus();
        });
        let target: &web_sys::EventTarget = overlay.as_ref();
        let _ = target.add_event_listener_with_callback_and_bool(
            "pointerdown",
            cb.as_ref().unchecked_ref(),
            true,
        );
        cb.forget();
    }
    let st = state.clone();
    let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
        let mut s = st.borrow_mut();
        if e.is_composing() || e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let left_hand = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
            .and_then(|el| el.get_attribute("data-keymap"))
            .as_deref()
            == Some("left-hand");
        let key = e.key().to_ascii_lowercase();
        if left_hand {
            let centre = (s.camera.width / 2.0, s.camera.height / 2.0);
            let handled = match key.as_str() {
                "w" => {
                    s.camera.pan(0.0, 40.0);
                    true
                }
                "a" => {
                    s.camera.pan(40.0, 0.0);
                    true
                }
                "s" => {
                    s.camera.pan(0.0, -40.0);
                    true
                }
                "d" => {
                    s.camera.pan(-40.0, 0.0);
                    true
                }
                "q" => {
                    s.camera.zoom_at(1.0 / 1.2, centre.0, centre.1);
                    true
                }
                "e" => {
                    s.camera.zoom_at(1.2, centre.0, centre.1);
                    true
                }
                "f" => {
                    let State { camera, graph, .. } = &mut *s;
                    camera.fit(graph, 40.0);
                    true
                }
                " " => {
                    s.space_pan = true;
                    true
                }
                _ => false,
            };
            if handled {
                s.auto_fit = false;
                s.dirty = true;
                e.prevent_default();
                e.stop_propagation();
                return;
            }
        }
        let n = s.graph.nodes.len();
        if n == 0 {
            return;
        }
        // The focused node's index — the focus is id-keyed, so a swapped
        // graph keeps it like the selection does.
        let idx = s
            .focus_id
            .as_deref()
            .and_then(|id| s.graph.nodes.iter().position(|nd| nd.id == id))
            .filter(|i| node_visible(&s.graph, *i));
        match e.key().as_str() {
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
                e.prevent_default(); // the page must not scroll
                let dir = match e.key().as_str() {
                    // Screen up is world −y in the 2D view.
                    "ArrowLeft" => [-1.0, 0.0],
                    "ArrowRight" => [1.0, 0.0],
                    "ArrowUp" => [0.0, -1.0],
                    _ => [0.0, 1.0],
                };
                // Nothing focused yet: walk from the map's first node —
                // the very first arrow already moves, it does not land
                // on node 0 itself.
                let target = match idx {
                    Some(i) => s.graph.nearest_in_direction(i, dir).or(Some(i)),
                    None => s.graph.nearest_in_direction(0, dir).or(Some(0)),
                };
                let Some(target) = target else { return };
                let id = s.graph.nodes[target].id.clone();
                if s.focus_id.as_deref() != Some(id.as_str()) {
                    s.focus_id = Some(id.clone());
                    s.dirty = true; // the focus ring redraws
                    emit(&s, serde_json::json!({ "kind": "focus", "id": id }));
                }
            }
            "Tab" if left_hand && !e.shift_key() => {
                let from = idx.unwrap_or(0);
                let next = s.graph.edges.iter().find_map(|edge| {
                    if edge.a == from {
                        Some(edge.b)
                    } else if edge.b == from {
                        Some(edge.a)
                    } else {
                        None
                    }
                });
                if let Some(next) = next {
                    let id = s.graph.nodes[next].id.clone();
                    s.focus_id = Some(id.clone());
                    s.dirty = true;
                    emit(&s, serde_json::json!({"kind":"focus","id":id}));
                    e.prevent_default();
                    e.stop_propagation();
                }
            }
            "Enter" => {
                if let Some(i) = idx {
                    e.prevent_default();
                    let id = s.graph.nodes[i].id.clone();
                    s.selected = frame::Selection {
                        nodes: [id.clone()].into_iter().collect(),
                        edges: Default::default(),
                    };
                    s.sel_rev += 1;
                    emit_select(&mut s);
                    emit(&s, serde_json::json!({ "kind": "click", "id": id }));
                }
            }
            "Escape" => {
                if !s.selected.is_empty() {
                    e.prevent_default();
                    s.selected = frame::Selection::default();
                    s.sel_rev += 1;
                    emit_select(&mut s);
                }
            }
            _ => {}
        }
    });
    let target: &web_sys::EventTarget = overlay.as_ref();
    let _ = target.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
    cb.forget();
    let key_state = state.clone();
    let release =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            if event.key() == " " {
                key_state.borrow_mut().space_pan = false;
            }
        });
    let _ = target.add_event_listener_with_callback("keyup", release.as_ref().unchecked_ref());
    release.forget();
    let blur = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        state.borrow_mut().space_pan = false;
    });
    let _ = target.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref());
    blur.forget();
}

#[wasm_bindgen]
impl GraphView {
    /// Replace the graph. When the new graph shares nodes with the current
    /// one (a reload after a save, a focus change, a resize of the panel —
    /// spec 017), those nodes keep their positions and pins, only new nodes
    /// are placed (next to their neighbours), the layout is warmed rather
    /// than restarted, and the camera is left alone. A graph with nothing
    /// in common starts fresh, as before.
    pub fn set_graph(&self, json: &str) -> Result<(), JsValue> {
        self.load(json, false)
    }

    /// Replace the graph and lay it out from scratch, ignoring the
    /// positions of the current one: a neighbourhood (the Local view) is a
    /// subset of the whole graph, and keeping the whole layout's positions
    /// scattered its few nodes across the map (Prompt26).
    pub fn set_graph_fresh(&self, json: &str) -> Result<(), JsValue> {
        self.load(json, true)
    }

    /// Load bounded plain engine JSON carrying e-class groups.
    pub fn set_egraph(&self, json: &str) -> Result<(), JsValue> {
        let egraph = crate::egraph::EGraph::parse(json).map_err(|e| JsValue::from_str(&e))?;
        self.set_graph_fresh(&serde_json::to_string(egraph.graph()).unwrap())?;
        self.state.borrow_mut().egraph = Some(egraph);
        Ok(())
    }

    /// Collapse/expand a class; empty id addresses all classes. Uses the
    /// ordinary aggregate/proxy apply_delta path, preserving the camera.
    pub fn collapse_eclass(&self, id: &str, collapsed: bool) -> Result<(), JsValue> {
        let delta = {
            let mut s = self.state.borrow_mut();
            let positions = s
                .graph
                .nodes
                .iter()
                .map(|n| (n.id.clone(), [n.x, n.y]))
                .collect();
            let egraph = s
                .egraph
                .as_mut()
                .ok_or_else(|| JsValue::from_str("no e-graph loaded"))?;
            egraph
                .collapse(id, collapsed, &positions)
                .map_err(|e| JsValue::from_str(&e))?
        };
        self.apply_delta(&serde_json::to_string(&delta).unwrap())?;
        let mut s = self.state.borrow_mut();
        s.auto_fit = false;
        if let Some(egraph) = &s.egraph {
            s.selected.nodes = s
                .selected
                .nodes
                .iter()
                .map(|id| egraph.visible_id(id))
                .collect();
        }
        emit_select(&mut s);
        Ok(())
    }

    /// E-class ids and labels for host controls.
    pub fn eclass_state(&self) -> String {
        serde_json::to_string(
            &self
                .state
                .borrow()
                .egraph
                .as_ref()
                .map(|e| e.classes())
                .unwrap_or_default(),
        )
        .unwrap()
    }

    /// The highlighted ancestry, separately from the user's selection.
    pub fn rewrite_state(&self) -> String {
        let s = self.state.borrow();
        let mut nodes: Vec<_> = s.rewrites.nodes.iter().collect();
        nodes.sort();
        let mut edges: Vec<_> = s.rewrites.edges.iter().collect();
        edges.sort();
        serde_json::json!({"nodes":nodes,"edges":edges}).to_string()
    }

    /// Apply an incremental change (spec 031 §4, stage 4a): add and remove
    /// nodes and edges **by instance id** — a collapse/expand or a live
    /// host sends this instead of re-serialising 100 k nodes through
    /// `set_graph`. Survivors keep their positions and pins; the camera
    /// and the id-keyed selection are untouched. The layout only warms
    /// (temperature 3), so the view drifts rather than restarts.
    pub fn apply_delta(&self, json: &str) -> Result<(), JsValue> {
        let d: InDelta =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut s = self.state.borrow_mut();
        // A node drag follows its node to its new index, or ends when the
        // node is gone — a stale index panicked every pointer move (#12,
        // P-155; the stage 4–6 review's #2).
        let dragged = s.dragged_id();
        if !s.graph.apply_delta(d) {
            return Ok(()); // nothing named existed; nothing to redraw
        }
        s.keep_drag(dragged);
        // Warm, not restart: newcomers settle next to where they were put.
        let mut l = Layout::new(&s.graph);
        l.temperature = 3.0;
        s.layout = l;
        // Indices shifted: the hover is an index, so it goes; the wire ends.
        s.hovered = None;
        s.hover_rev += 1;
        end_wire(&mut s);
        s.pos_rev += 1;
        s.attr_rev += 1;
        s.animates = animates(&s.graph);
        s.has_icons = has_ink(&s.graph);
        s.has_edge_labels = s.graph.edges.iter().any(|e| !e.label.is_empty());
        s.dirty = true;
        Ok(())
    }

    /// Put the current view aside under `name` (graph, positions, pins,
    /// layout state, camera).
    pub fn stash(&self, name: &str) {
        let mut s = self.state.borrow_mut();
        let saved = Stashed {
            graph: s.graph.clone(),
            layout: s.layout,
            camera: s.camera,
            auto_fit: s.auto_fit,
        };
        s.stashed.insert(name.to_string(), saved);
    }

    /// Bring back the view stashed under `name`, exactly as it was; `false`
    /// if there is none. A settled layout stays settled.
    pub fn unstash(&self, name: &str) -> bool {
        let mut s = self.state.borrow_mut();
        let Some(saved) = s.stashed.remove(name) else {
            return false;
        };
        let dragged = s.dragged_id();
        s.graph = saved.graph;
        s.keep_drag(dragged);
        s.layout = saved.layout;
        s.camera = saved.camera;
        s.auto_fit = saved.auto_fit;
        s.hovered = None;
        s.pos_rev += 1;
        s.attr_rev += 1;
        s.animates = animates(&s.graph);
        s.has_icons = has_ink(&s.graph);
        s.has_edge_labels = s.graph.edges.iter().any(|e| !e.label.is_empty());
        s.dirty = true;
        true
    }

    /// Timings of the last full load in milliseconds: JSON parse, graph build,
    /// installation/fit, first layout, first draw submission, load-to-first-draw.
    /// The final entry remains zero until that graph has been submitted.
    pub fn load_stats(&self) -> Vec<f64> {
        self.state.borrow().load_stats.to_vec()
    }

    fn load(&self, json: &str, fresh: bool) -> Result<(), JsValue> {
        let started = clock_ms();
        let input: InGraph =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut s = self.state.borrow_mut();
        s.egraph = None;
        s.rewrites = frame::Selection::default();
        let parsed = clock_ms();
        s.load_started = started;
        s.awaiting_first_draw = true;
        s.load_stats = [0.0; 6];
        s.load_stats[0] = parsed - started;
        let dragged = s.dragged_id();
        // A **pinned** node with an explicit position is the host's to move —
        // the document owns a pinned block's coordinates (stage 7c: undo
        // restores them); an unpinned one keeps its live position (the
        // explorer's layout wins).
        let host_moved: std::collections::HashSet<String> = input
            .nodes
            .iter()
            .filter(|n| n.pinned && n.pos.is_some())
            .map(|n| n.id.clone())
            .collect();
        let mut next = Graph::from_input(input);
        next.grid = s.graph.grid;
        let built = clock_ms();
        s.load_stats[1] = built - parsed;
        let old: std::collections::HashMap<String, (f32, f32, f32, bool)> = s
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.clone(), (n.x, n.y, n.z, n.pinned)))
            .collect();
        let shared = next
            .nodes
            .iter()
            .filter(|n| old.contains_key(&n.id))
            .count();
        let incremental = !fresh && !old.is_empty() && shared * 2 >= next.nodes.len().max(1);
        if incremental {
            // Known nodes stay put; new ones start at the mean of their known
            // neighbours (or where the spiral put them) with a nudge so two
            // new siblings do not coincide — unless the host repositioned a
            // pinned node (see `host_moved` above).
            let known: Vec<bool> = next.nodes.iter().map(|n| old.contains_key(&n.id)).collect();
            let mut placed: Vec<Option<(f32, f32, f32)>> = vec![None; next.nodes.len()];
            for (i, n) in next.nodes.iter().enumerate() {
                if let Some(&(x, y, z, _)) = old.get(&n.id) {
                    placed[i] = if host_moved.contains(&n.id) {
                        Some((n.x, n.y, n.z))
                    } else {
                        Some((x, y, z))
                    };
                }
            }
            for (i, n) in next.nodes.iter().enumerate() {
                // A pinned newcomer sits where the host put it (a flow
                // block, stage 7) — the neighbour mean is for explorers.
                if known[i] || n.pinned {
                    continue;
                }
                let mut acc = (0.0f32, 0.0f32, 0.0f32);
                let mut count = 0.0f32;
                for e in &next.edges {
                    let other = if e.a == i {
                        e.b
                    } else if e.b == i {
                        e.a
                    } else {
                        continue;
                    };
                    if let Some((x, y, z)) = placed[other] {
                        acc.0 += x;
                        acc.1 += y;
                        acc.2 += z;
                        count += 1.0;
                    }
                }
                if count > 0.0 {
                    let t = i as f32 * 2.399_963;
                    placed[i] = Some((
                        acc.0 / count + 18.0 * t.cos(),
                        acc.1 / count + 18.0 * t.sin(),
                        n.z,
                    ));
                }
            }
            let mut changed = next.nodes.len() != old.len();
            for (i, n) in next.nodes.iter_mut().enumerate() {
                match placed[i] {
                    Some((x, y, z)) => {
                        n.x = x;
                        n.y = y;
                        n.z = z;
                        if let Some(&(_, _, _, pinned)) = old.get(&n.id) {
                            n.pinned = pinned;
                        }
                    }
                    None => changed = true,
                }
                if !known[i] {
                    changed = true;
                }
            }
            let same_edges = next.edges.len() == s.graph.edges.len();
            let new_count = known.iter().filter(|k| !**k).count();
            let total = next.nodes.len().max(1);
            s.graph = next;
            s.keep_drag(dragged);
            if changed || !same_edges {
                // A few newcomers: warm, so known nodes only drift. Many (an
                // index still filling up): a full reheat from the kept
                // positions — still no camera reset.
                let mut l = Layout::new(&s.graph);
                if new_count * 4 < total {
                    l.temperature = 3.0;
                }
                s.layout = l;
            }
            s.hovered = None;
            s.pos_rev += 1;
            s.attr_rev += 1;
            s.animates = animates(&s.graph);
            s.has_icons = has_ink(&s.graph);
            s.has_edge_labels = s.graph.edges.iter().any(|e| !e.label.is_empty());
            s.dirty = true;
            s.load_stats[2] = clock_ms() - built;
            return Ok(());
        }
        s.graph = next;
        s.keep_drag(dragged);
        s.layout = Layout::new(&s.graph);
        s.hovered = None;
        s.auto_fit = true;
        s.pos_rev += 1;
        s.attr_rev += 1;
        s.animates = animates(&s.graph);
        s.has_icons = has_ink(&s.graph);
        s.has_edge_labels = s.graph.edges.iter().any(|e| !e.label.is_empty());
        // A rough fit up front so the first frames are on screen; fit again when settled.
        let g = std::mem::take(&mut s.graph);
        s.camera.fit(&g, 40.0);
        s.graph = g;
        s.dirty = true;
        s.load_stats[2] = clock_ms() - built;
        Ok(())
    }

    /// Follow the theme (spec 030): the background as `#rrggbb`, and the
    /// label colours as any CSS colour. A background that is not `#rrggbb`
    /// keeps the current one.
    pub fn set_theme(&self, background: &str, label: &str, label_hover: &str) {
        let mut s = self.state.borrow_mut();
        if let Some([r, g, b, _]) = crate::graph::parse_hex(background.trim(), 1.0) {
            s.renderer.clear = [r as f64, g as f64, b as f64];
        }
        if !label.trim().is_empty() {
            s.label = label.trim().to_string();
        }
        if !label_hover.trim().is_empty() {
            s.label_hover = label_hover.trim().to_string();
        }
        s.dirty = true;
    }

    /// Limit automatic fit magnification to leave space for host components.
    /// Explicit wheel/pinch zoom remains unchanged. Non-finite limits are ignored.
    pub fn set_fit_scale_limit(&self, limit: f32) {
        if limit.is_finite() {
            self.state.borrow_mut().camera.max_fit_scale =
                limit.clamp(crate::camera::MIN_SCALE, 12.0);
        }
    }

    pub fn fit(&self) {
        let mut s = self.state.borrow_mut();
        let g = std::mem::take(&mut s.graph);
        s.camera.fit(&g, 40.0);
        s.graph = g;
        s.dirty = true;
    }

    /// Fit to a subset (spec 031 §4: layouts centred on one or several
    /// focal nodes) — a JSON array of node ids. An explicit focus is a
    /// navigation act: it ends the auto-fit, unlike a whole-graph fit.
    pub fn focus(&self, ids: &str) {
        let ids: Vec<String> = match serde_json::from_str(ids) {
            Ok(ids) => ids,
            Err(_) => return,
        };
        let mut s = self.state.borrow_mut();
        let (mut x0, mut y0) = (f32::MAX, f32::MAX);
        let (mut x1, mut y1) = (f32::MIN, f32::MIN);
        let (mut z0, mut z1) = (f32::MAX, f32::MIN);
        let mut any = false;
        for n in s.graph.nodes.iter().filter(|n| ids.contains(&n.id)) {
            x0 = x0.min(n.x - n.radius);
            y0 = y0.min(n.y - n.radius);
            x1 = x1.max(n.x + n.radius);
            y1 = y1.max(n.y + n.radius);
            z0 = z0.min(n.z);
            z1 = z1.max(n.z);
            any = true;
        }
        if !any {
            return;
        }
        // The rect carries the nodes' rims; the pad is pixels, like `fit`
        // — and 3D fits too, via the shared `fit_rect` (the stage 4–6
        // review's #9: this re-derived the 2D half only, in world units).
        s.camera.fit_rect((x0, y0, x1, y1), (z0, z1), 40.0);
        s.auto_fit = false;
        s.dirty = true;
    }

    /// Keep the current camera fixed during subsequent layout/delta work.
    /// Explicit fit/relayout commands can enable automatic fitting again.
    pub fn hold_camera(&self) {
        self.state.borrow_mut().auto_fit = false;
    }

    /// The layered arrangement (spec 031 SS5, stage 7): every wire left
    /// to right, columns and rows — the flow editor's Layout button. The
    /// nodes land pinned (a flow's positions are the host's document);
    /// the host harvests them with [`node_positions`](Self::node_positions).
    pub fn hier_layout(&self) {
        let mut s = self.state.borrow_mut();
        s.graph.hierarchize();
        s.layout.running = false; // the arrangement is final, not settling
        s.auto_fit = true;
        let g = std::mem::take(&mut s.graph);
        s.camera.fit(&g, 40.0); // the whole arrangement on screen
        s.graph = g;
        s.pos_rev += 1;
        s.dirty = true;
    }

    /// Every node's world position as `{"id":[x,y],...}` — the host's
    /// harvest after a drag or a layout (stage 7: the flow document owns
    /// the positions, the renderer only holds them between gestures).
    pub fn node_positions(&self) -> String {
        let s = self.state.borrow();
        let mut out = String::from("{");
        for (i, n) in s.graph.nodes.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&serde_json::to_string(&n.id).unwrap_or_default());
            out.push_str(&format!(":[{:.3},{:.3}]", n.x, n.y));
        }
        out.push('}');
        out
    }

    pub fn relayout(&self) {
        let mut s = self.state.borrow_mut();
        s.auto_fit = true;
        for n in &mut s.graph.nodes {
            n.pinned = false;
        }
        let g = std::mem::take(&mut s.graph);
        s.layout.reheat(&g);
        s.graph = g;
        s.dirty = true;
    }

    pub fn resize(&self, width: f32, height: f32, dpr: f32) {
        let mut s = self.state.borrow_mut();
        s.camera.width = width.max(1.0);
        s.camera.height = height.max(1.0);
        s.dpr = dpr;
        s.renderer
            .resize((width * dpr) as u32, (height * dpr) as u32);
        let canvas = s.overlay.canvas();
        if let Some(c) = canvas {
            c.set_width((width * dpr) as u32);
            c.set_height((height * dpr) as u32);
        }
        // A view nobody has panned or zoomed yet follows the canvas: the
        // first real size may arrive after the layout settled (P-091, a
        // hidden phone tile), and a resized panel should stay fitted.
        if s.auto_fit && !s.layout.running {
            let g = std::mem::take(&mut s.graph);
            s.camera.fit(&g, 40.0);
            s.graph = g;
        }
        s.dirty = true;
    }

    pub fn backend(&self) -> String {
        self.state.borrow().renderer.backend.clone()
    }

    /// Set world grid spacing (zero disables it), bounded to useful canvas sizes.
    pub fn set_grid(&self, spacing: f32) {
        let mut s = self.state.borrow_mut();
        s.graph.grid = if spacing.is_finite() && (4.0..=128.0).contains(&spacing) {
            spacing
        } else {
            0.0
        };
        s.pos_rev += 1;
        s.dirty = true;
    }

    /// `"explore"` or `"edit"` (spec 031 §5): the same renderer, two
    /// interaction modes — edit draws the ports and lets a wire be dragged
    /// between them; the camera is never touched. `set_mode` stays the
    /// 2D/3D switch.
    pub fn set_interaction(&self, mode: &str) {
        let mut s = self.state.borrow_mut();
        let edit = mode == "edit";
        let next = if edit {
            Interaction::Edit
        } else {
            Interaction::Explore
        };
        if s.interaction == next {
            return;
        }
        s.interaction = next;
        s.wire_from = None;
        s.wire_to = None;
        s.pos_rev += 1; // ports join or leave the node instances
        s.attr_rev += 1;
        s.dirty = true;
    }

    /// Show or hide a layer by id (spec 031 §3): presentation only — the
    /// graph's topology is never touched, and selection by id survives.
    pub fn set_layer_visibility(&self, id: &str, visible: bool) {
        let mut s = self.state.borrow_mut();
        if let Some(l) = s.graph.layers.iter_mut().find(|l| l.id == id) {
            if l.visible != visible {
                l.visible = visible;
                s.pos_rev += 1; // segments and nodes appear or fade
                s.attr_rev += 1;
                s.dirty = true;
            }
        }
    }

    /// `[{"id":…,"visible":…}, …]` — the layers and their visibility, for
    /// tests.
    pub fn layer_state(&self) -> String {
        let s = self.state.borrow();
        let layers: Vec<serde_json::Value> = s
            .graph
            .layers
            .iter()
            .map(|l| serde_json::json!({ "id": l.id, "visible": l.visible, "overlay": l.overlay }))
            .collect();
        serde_json::json!(layers).to_string()
    }

    /// `"2d"` or `"3d"` (Milestone 8): the same graph, a perspective camera
    /// orbiting the layout with one plane per node kind.
    pub fn set_mode(&self, mode: &str) {
        let mut s = self.state.borrow_mut();
        let three_d = mode == "3d";
        if s.camera.three_d != three_d {
            s.camera.three_d = three_d;
            s.hovered = None;
            s.attr_rev += 1;
            s.hover_rev += 1;
            s.pos_rev += 1; // arrow and segment positions re-upload (B6)
            let g = std::mem::take(&mut s.graph);
            s.camera.fit(&g, 40.0);
            s.graph = g;
            s.dirty = true;
        }
    }

    pub fn mode(&self) -> String {
        if self.state.borrow().camera.three_d {
            "3d".into()
        } else {
            "2d".into()
        }
    }

    pub fn node_count(&self) -> usize {
        self.state.borrow().graph.nodes.len()
    }

    /// Edge count, for the delta tests (spec 031 §1).
    pub fn edge_count(&self) -> usize {
        self.state.borrow().graph.edges.len()
    }

    /// `[running, temperature, iterations, x0, y0, x1, y1]` — the layout, for tests.
    pub fn layout_state(&self) -> Vec<f32> {
        let s = self.state.borrow();
        let (x0, y0, x1, y1) = s.graph.bounds().unwrap_or((0.0, 0.0, 0.0, 0.0));
        vec![
            if s.layout.running { 1.0 } else { 0.0 },
            s.layout.temperature,
            s.layout.iterations as f32,
            x0,
            y0,
            x1,
            y1,
        ]
    }

    /// `[scale, cx, cy, yaw, pitch, dist]` — the camera, for tests (spec 006).
    pub fn camera_state(&self) -> Vec<f32> {
        let c = &self.state.borrow().camera;
        vec![c.scale, c.cx, c.cy, c.yaw, c.pitch, c.dist]
    }

    /// Navigation snapshot `[scale, cx, cy, cz, yaw, pitch, dist]`.
    /// Includes depth while retaining the original six-value `camera_state` API.
    pub fn navigation_camera_state(&self) -> Vec<f32> {
        let c = &self.state.borrow().camera;
        vec![c.scale, c.cx, c.cy, c.cz, c.yaw, c.pitch, c.dist]
    }

    /// Restore a bounded, finite camera snapshot from session navigation.
    pub fn restore_camera(&self, values: &[f32]) -> bool {
        if values.len() != 7
            || values.iter().any(|x| !x.is_finite())
            || !(crate::camera::MIN_SCALE..=crate::camera::MAX_SCALE).contains(&values[0])
            || values[6] <= 0.0
        {
            return false;
        }
        let mut state = self.state.borrow_mut();
        state.camera.scale = values[0];
        state.camera.cx = values[1];
        state.camera.cy = values[2];
        state.camera.cz = values[3];
        state.camera.yaw = values[4];
        state.camera.pitch = values[5];
        state.camera.dist = values[6];
        state.auto_fit = false;
        state.dirty = true;
        true
    }

    /// Set a host-linked selection by existing instance ids, without emitting
    /// an event back to the host. Bounded input prevents feedback loops and
    /// keeps unrelated or stale identities out of the selection.
    pub fn select_nodes(&self, json: &str) -> bool {
        if json.len() > 1024 * 1024 {
            return false;
        }
        let Ok(ids) = serde_json::from_str::<Vec<String>>(json) else {
            return false;
        };
        if ids.len() > 4096 || ids.iter().any(|id| id.len() > 256) {
            return false;
        }
        let mut s = self.state.borrow_mut();
        let available: std::collections::HashSet<_> =
            s.graph.nodes.iter().map(|n| n.id.as_str()).collect();
        let nodes = ids
            .into_iter()
            .filter(|id| available.contains(id.as_str()))
            .collect();
        let selection = frame::Selection {
            nodes,
            edges: Default::default(),
        };
        if selection != s.selected {
            s.selected = selection;
            s.sel_rev += 1;
            s.dirty = true;
        }
        true
    }

    /// The current selection as `{"nodes":[…],"edges":[…]}` — for tests
    /// (spec 031 §5).
    pub fn selection_state(&self) -> String {
        let s = self.state.borrow();
        let mut nodes: Vec<&String> = s.selected.nodes.iter().collect();
        nodes.sort();
        let mut edges: Vec<&String> = s.selected.edges.iter().collect();
        edges.sort();
        serde_json::json!({ "nodes": nodes, "edges": edges }).to_string()
    }

    /// `[segments, arrows, ports, hulls, simplexes, junctions]` — instance
    /// counts; ports count only in edit mode (they are drawn then), hulls
    /// and simplexes are 2D-only, junctions ride the node pipeline — for
    /// tests (spec 031 §2, §4, stage 5).
    pub fn frame_state(&self) -> Vec<f64> {
        let s = self.state.borrow();
        let f = s.renderer.frame();
        let ports = if s.interaction == Interaction::Edit {
            f.port_pos.len()
        } else {
            0
        };
        vec![
            f.segs.len() as f64,
            f.arrows.len() as f64,
            ports as f64,
            f.hulls.len() as f64,
            f.simplex_tris.len() as f64,
            f.junctions.len() as f64,
        ]
    }

    /// `"explore"` or `"edit"` — the interaction mode (spec 031 §5), for
    /// tests. `set_mode` stays the 2D/3D switch (Milestone 8).
    pub fn interaction(&self) -> String {
        match self.state.borrow().interaction {
            Interaction::Edit => "edit".into(),
            Interaction::Explore => "explore".into(),
        }
    }

    /// The last `connect` event's JSON (`{"from":null}` before any) — for
    /// tests.
    pub fn connect_state(&self) -> String {
        self.state
            .borrow()
            .last_connect
            .clone()
            .unwrap_or_else(|| r#"{"from":null}"#.into())
    }

    /// A port's screen position by node id and port name — for tests and
    /// for the host's port popups.
    pub fn port_screen_position(&self, node: &str, port: &str) -> Option<Vec<f32>> {
        let s = self.state.borrow();
        let n = s.graph.nodes.iter().position(|n| n.id == node)?;
        let p = s
            .graph
            .ports
            .iter()
            .position(|p| p.node == n && p.name == port)?;
        let pw = frame::port_world(&s.graph, &s.graph.ports[p]);
        if s.camera.three_d {
            let (x, y, _) = s.camera.project(pw[0], pw[1], pw[2])?;
            Some(vec![x, y])
        } else {
            let (x, y) = s.camera.world_to_screen(pw[0], pw[1]);
            Some(vec![x, y])
        }
    }

    /// Screen position of a node by id (for tests and for the host's popup).
    pub fn node_screen_position(&self, id: &str) -> Option<Vec<f32>> {
        let s = self.state.borrow();
        let i = s.graph.nodes.iter().position(|n| n.id == id)?;
        let n = &s.graph.nodes[i];
        let (x, y) = if s.camera.three_d {
            let (x, y, _) = s.camera.project(n.x, n.y, n.z)?;
            (x, y)
        } else {
            s.camera.world_to_screen(n.x, n.y)
        };
        Some(vec![x, y])
    }

    pub fn destroy(&self) {
        self.state.borrow_mut().alive = false;
    }
}

/// Does any edge carry a pulse (spec 031 §2, stage 6)? The view redraws
/// every frame while true — the clock moves even when nothing is dirty.
fn animates(g: &Graph) -> bool {
    g.edges.iter().any(|e| e.pulse_speed > 0.0)
}

fn clock_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn start_loop(state: Rc<RefCell<State>>) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    let st = state.clone();
    *g.borrow_mut() = Some(Closure::new(move || {
        let mut s = st.borrow_mut();
        if !s.alive {
            return;
        }
        // Submit the fitted initial positions before spending a layout step
        // on them. At 100k nodes that step alone can block a first paint.
        // Subsequent frames retain the existing fit-while-settling policy.
        if s.layout.running && !s.awaiting_first_draw {
            let layout_started = clock_ms();
            let n = s.graph.nodes.len();
            let iters = if n < 300 {
                4
            } else if n < 1500 {
                2
            } else {
                1
            };
            let mut g = std::mem::take(&mut s.graph);
            for _ in 0..iters {
                s.layout.step(&mut g);
            }
            s.graph = g;
            if s.load_stats[3] == 0.0 {
                s.load_stats[3] = clock_ms() - layout_started;
            }
            s.pos_rev += 1;
            s.dirty = true;
            // An untouched view follows the layout while it spreads (O(n)
            // per frame, nothing next to a layout step), so a big graph is
            // never off screen while it settles; the last fit is exact.
            if s.auto_fit {
                let g = std::mem::take(&mut s.graph);
                s.camera.fit(&g, 40.0);
                s.graph = g;
            }
            if !s.layout.running {
                emit(&s, serde_json::json!({ "kind": "settled" }));
            }
        }
        // The pulse clock (spec 031 §2, stage 6): seconds since the view
        // was created. A pulsed view redraws every frame — the animation
        // is presentation in the shader, nothing on the CPU moves.
        let now_ms = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(s.t0_ms);
        s.camera.time = ((now_ms - s.t0_ms) / 1000.0) as f32;
        if s.dirty || s.animates {
            s.dirty = false;
            let t0 = web_sys::window()
                .and_then(|w| w.performance())
                .map(|p| p.now());
            let pending = match (&s.wire_from, s.wire_to) {
                (Some((node, name)), Some(t)) => port_index(&s.graph, node, name)
                    .map(|p| (frame::port_world(&s.graph, &s.graph.ports[p]), t)),
                _ => None,
            };
            let edit = s.interaction == Interaction::Edit;
            // Ordinary graphs keep borrowing selection, including large selections.
            let highlighted = (!s.rewrites.is_empty()).then(|| {
                let mut highlighted = s.selected.clone();
                highlighted.nodes.extend(s.rewrites.nodes.iter().cloned());
                highlighted.edges.extend(s.rewrites.edges.iter().cloned());
                highlighted
            });
            let State {
                graph,
                camera,
                renderer,
                hovered,
                selected,
                pos_rev,
                attr_rev,
                hover_rev,
                sel_rev,
                ..
            } = &mut *s;
            let input = frame::DrawInput {
                camera,
                hovered: *hovered,
                selected: highlighted.as_ref().unwrap_or(selected),
                edit,
                pending,
                pos_rev: *pos_rev,
                attr_rev: *attr_rev,
                hover_rev: *hover_rev,
                sel_rev: *sel_rev,
            };
            renderer.draw(graph, &input);
            if let Some(t0) = t0 {
                let t1 = web_sys::window()
                    .and_then(|w| w.performance())
                    .map(|p| p.now())
                    .unwrap_or(t0);
                DRAW_MICROS.fetch_add(
                    ((t1 - t0) * 1000.0) as u64,
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
            DRAW_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            draw_labels(&s);
            if s.awaiting_first_draw {
                s.load_stats[4] = clock_ms() - t0.unwrap_or_else(clock_ms);
                s.load_stats[5] = clock_ms() - s.load_started;
                s.awaiting_first_draw = false;
            }
        }
        let camera = s.camera.component_transform();
        if s.reported_camera != Some(camera) {
            s.reported_camera = Some(camera);
            emit(
                &s,
                serde_json::json!({"kind":"camera", "scale":camera[0],
                "offset":[camera[1],camera[2]], "width":camera[3], "height":camera[4],
                "threeD":camera[5] != 0.0}),
            );
        }
        drop(s);
        request_frame(f.borrow().as_ref().unwrap());
    }));
    request_frame(g.borrow().as_ref().unwrap());
}

fn request_frame(f: &Closure<dyn FnMut()>) {
    if let Some(w) = web_sys::window() {
        let _ = w.request_animation_frame(f.as_ref().unchecked_ref());
    }
}

/// Any node with overlay ink — an icon or a symbol (ADR-0024).
fn has_ink(g: &Graph) -> bool {
    g.nodes
        .iter()
        .any(|n| n.icon.is_some() || n.symbol.is_some())
}

fn draw_labels(s: &State) {
    let ctx = &s.overlay;
    let (w, h) = (
        s.camera.width as f64 * s.dpr as f64,
        s.camera.height as f64 * s.dpr as f64,
    );
    ctx.clear_rect(0.0, 0.0, w, h);
    let _ = ctx.reset_transform();
    let _ = ctx.scale(s.dpr as f64, s.dpr as f64);
    let scale = s.camera.scale;
    if s.graph.grid > 0.0 && !s.camera.three_d {
        let spacing = s.graph.grid * scale;
        // Keep work bounded at distant zoom and extreme viewports.
        if spacing >= 8.0 {
            let (ox, oy) = s.camera.world_to_screen(0.0, 0.0);
            ctx.save();
            ctx.set_global_alpha(0.12);
            ctx.set_fill_style_str(&s.label_hover);
            let mut x = ox.rem_euclid(spacing);
            for _ in 0..2048 {
                if x > s.camera.width {
                    break;
                }
                let mut y = oy.rem_euclid(spacing);
                for _ in 0..2048 {
                    if y > s.camera.height {
                        break;
                    }
                    ctx.fill_rect(x as f64, y as f64, 1.0, 1.0);
                    y += spacing;
                }
                x += spacing;
            }
            ctx.restore();
        }
    }
    // Labels only when there's room: zoomed in, or few nodes. Past 20k nodes
    // the overlay scan itself costs frames: hovered label only.
    let huge = s.graph.nodes.len() > 20_000;
    let show_all = !huge && ((!s.camera.three_d && scale >= 0.9) || s.graph.nodes.len() <= 60);
    ctx.set_font("12px 'Segoe UI', sans-serif");
    ctx.set_text_baseline("middle");
    // Icons (spec 031 §2, stage 8): a glyph centered on the node — but
    // only while hovered or selected (§6 reserves per-node ink for the
    // few), and only when the projected size can hold it. The scan is a
    // second pass over every node per frame, so it waits until some node
    // actually carries an icon.
    // Symbols (ADR-0024): a node's schematic, stroked over its faint
    // body — the symbol is the block. Its box is the node's (`0 0 2·r
    // 2·h` world units), so it zooms with the canvas; the stroke stays
    // ~1.6 px. Hovered or selected, it is inked in the hover colour.
    if s.has_icons && !s.camera.three_d {
        ctx.set_line_join("round");
        ctx.set_line_cap("round");
        for (i, n) in s.graph.nodes.iter().enumerate() {
            if n.embed {
                continue;
            }
            let Some(d) = n.symbol.as_deref() else {
                continue;
            };
            if d.len() > 65536 {
                continue;
            }
            if !node_visible(&s.graph, i) {
                continue;
            }
            let (x, y) = s.camera.world_to_screen(n.x, n.y);
            let (hw, hh) = (n.radius * scale, n.height * scale);
            if x + hw < 0.0 || y + hh < 0.0 || x - hw > s.camera.width || y - hh > s.camera.height {
                continue;
            }
            if hw < 4.0 {
                continue; // LOD: the faint body is all a speck needs
            }
            let Ok(path) = web_sys::Path2d::new_with_path_string(d) else {
                continue;
            };
            let lit = Some(i) == s.hovered || s.selected.nodes.contains(&n.id);
            let ink = if lit {
                s.label_hover.clone()
            } else {
                format!(
                    "rgb({:.0},{:.0},{:.0})",
                    n.color[0] * 255.0,
                    n.color[1] * 255.0,
                    n.color[2] * 255.0
                )
            };
            // The path lives in the unturned box; the node's box is the
            // turned one (ADR-0024 §5). Centre, turn, mirror, back to
            // the unturned box's corner.
            let (bw, bh) = if n.turn % 2 == 1 {
                (2.0 * n.height, 2.0 * n.radius)
            } else {
                (2.0 * n.radius, 2.0 * n.height)
            };
            ctx.save();
            let _ = ctx.translate(x as f64, y as f64);
            let _ = ctx.rotate(n.turn as f64 * std::f64::consts::FRAC_PI_2);
            let flip = if n.mirror { -1.0 } else { 1.0 };
            let _ = ctx.scale(flip * scale as f64, scale as f64);
            let _ = ctx.translate(-bw as f64 / 2.0, -bh as f64 / 2.0);
            ctx.set_stroke_style_str(&ink);
            ctx.set_line_width((if lit { 2.2 } else { 1.6 }) / scale as f64);
            ctx.set_fill_style_str(&ink);
            for fill in crate::graph::filled_regions(d).take(256) {
                if let Ok(region) = web_sys::Path2d::new_with_path_string(fill) {
                    ctx.fill_with_path_2d(&region);
                }
            }
            ctx.stroke_with_path(&path);
            ctx.restore();
        }
    }
    if s.has_icons {
        for (i, n) in s.graph.nodes.iter().enumerate() {
            if n.embed {
                continue;
            }
            let Some(icon) = n.icon.as_deref() else {
                continue;
            };
            let hovered = Some(i) == s.hovered;
            if !hovered && !s.selected.nodes.contains(&n.id) {
                continue;
            }
            if !node_visible(&s.graph, i) {
                continue; // a layer-hidden node keeps its ink off too
            }
            let (x, y, r) = if s.camera.three_d {
                let Some((x, y, w)) = s.camera.project(n.x, n.y, n.z) else {
                    continue;
                };
                (x, y, n.radius * s.camera.size_factor(w))
            } else {
                let (x, y) = s.camera.world_to_screen(n.x, n.y);
                (x, y, n.radius * scale.max(0.6))
            };
            if r < 7.0 {
                continue; // LOD: a speck of ink is noise
            }
            // The ink contrasts with the node's own fill: a bright fill (the
            // hovered node is white) wears the background's ink, a dark one
            // the theme's label ink.
            let lum = 0.2126 * n.color[0] + 0.7152 * n.color[1] + 0.0722 * n.color[2];
            let bg_ink = format!(
                "rgb({:.0},{:.0},{:.0})",
                s.renderer.clear[0] * 255.0,
                s.renderer.clear[1] * 255.0,
                s.renderer.clear[2] * 255.0
            );
            let ink = if lum > 0.5 { bg_ink } else { s.label.clone() };
            ctx.set_text_align("center");
            ctx.set_fill_style_str(&ink);
            let _ = ctx.fill_text(icon, x as f64, y as f64);
            ctx.set_text_align("start");
        }
    }
    // An edge's label rides the pointer while its stroke is hovered
    // (spec 031 §2, stage 9), inked in the edge's own colour so it
    // names the thing it sits on.
    if let Some(i) = s.hovered_edge {
        if let Some(e) = s.graph.edges.get(i) {
            if !e.label.is_empty() {
                ctx.set_fill_style_str(&format!(
                    "rgb({:.0},{:.0},{:.0})",
                    e.color[0] * 255.0,
                    e.color[1] * 255.0,
                    e.color[2] * 255.0
                ));
                let _ = ctx.fill_text(
                    &e.label,
                    (s.pointer.0 + 12.0) as f64,
                    (s.pointer.1 - 10.0) as f64,
                );
            }
        }
    }
    // The keyboard focus wears a ring (spec 031, stage 10): without it a
    // keyboard user cannot see where Tab or the arrows landed.
    if let Some(id) = &s.focus_id {
        if let Some(i) = s.graph.nodes.iter().position(|n| &n.id == id) {
            if node_visible(&s.graph, i) {
                let n = &s.graph.nodes[i];
                let (x, y, r) = if s.camera.three_d {
                    match s.camera.project(n.x, n.y, n.z) {
                        Some((x, y, w)) => (x, y, n.radius * s.camera.size_factor(w)),
                        None => (0.0, 0.0, 0.0),
                    }
                } else {
                    let (x, y) = s.camera.world_to_screen(n.x, n.y);
                    (x, y, n.radius * s.camera.scale.max(0.6))
                };
                if r > 0.0 {
                    ctx.set_stroke_style_str(&s.label_hover);
                    ctx.set_line_width(1.5);
                    ctx.begin_path();
                    let _ = ctx.arc(x as f64, y as f64, (r + 5.0) as f64, 0.0, 6.2832);
                    let _ = ctx.stroke();
                }
            }
        }
    }
    // Candidates by importance (hovered first, then degree), placed with a
    // screen-space collision test so labels never pile onto each other in a
    // dense cluster (spec 017 follow-up): the important ones win the space.
    let mut candidates: Vec<(usize, f32, f32, f32)> = Vec::new();
    for (i, n) in s.graph.nodes.iter().enumerate() {
        if n.embed {
            continue;
        }
        let hovered = Some(i) == s.hovered;
        if !show_all && !hovered && (huge || n.degree < 3) {
            continue;
        }
        if !node_visible(&s.graph, i) {
            continue; // a layer-hidden node keeps its label off too
        }
        let (x, y, r) = if s.camera.three_d {
            let Some((x, y, w)) = s.camera.project(n.x, n.y, n.z) else {
                continue;
            };
            // Far-away labels only when hovered (fog hides the node anyway).
            if !hovered && w > s.camera.dist * 1.6 {
                continue;
            }
            (x, y, n.radius * s.camera.size_factor(w))
        } else {
            let (x, y) = s.camera.world_to_screen(n.x, n.y);
            (x, y, n.radius * scale.max(0.6))
        };
        if x < -100.0 || y < -20.0 || x > s.camera.width + 100.0 || y > s.camera.height + 20.0 {
            continue;
        }
        candidates.push((i, x, y, r));
    }
    candidates.sort_by(|a, b| {
        let ha = Some(a.0) == s.hovered;
        let hb = Some(b.0) == s.hovered;
        hb.cmp(&ha)
            .then_with(|| s.graph.nodes[b.0].degree.cmp(&s.graph.nodes[a.0].degree))
    });
    let mut placed: Vec<(f32, f32, f32, f32)> = Vec::new(); // x0, y0, x1, y1
    let mut drawn = 0;
    for (i, x, y, r) in candidates {
        let hovered = Some(i) == s.hovered;
        if drawn > 400 && !hovered {
            break;
        }
        let n = &s.graph.nodes[i];
        let width = ctx
            .measure_text(&n.label)
            .map(|m| m.width() as f32)
            .unwrap_or(n.label.len() as f32 * 6.5);
        // A symbol's label sits centred below it, as on a schematic; any
        // other node's to its right.
        let symbol = n.symbol.is_some() && !s.camera.three_d;
        // A symbol with a pin on its bottom (an upright part) would put
        // its label across the wire leaving there: the label moves to
        // its right, centred on it (ADR-0024 §5).
        let beside = symbol
            && s.graph
                .ports
                .iter()
                .any(|p| p.node == i && p.side == crate::graph::InSide::Bottom);
        let symbol = symbol && !beside;
        let (x0, y) = if symbol {
            (x - width / 2.0, y + n.height * scale + 10.0)
        } else if beside {
            let lift = if n.caption.is_some() { 7.0 } else { 0.0 };
            (x + n.radius * scale + 8.0, y - lift)
        } else {
            (x + r + 4.0, y)
        };
        // A caption (ADR-0024 §6) is a second, smaller line under the
        // label, centred under a symbol like the label.
        let caption = n.caption.as_deref().filter(|c| !c.is_empty());
        let cap_width = caption
            .map(|c| {
                ctx.set_font("11px 'Segoe UI', sans-serif");
                let w = ctx
                    .measure_text(c)
                    .map(|m| m.width() as f32)
                    .unwrap_or(c.len() as f32 * 6.0);
                ctx.set_font("12px 'Segoe UI', sans-serif");
                w
            })
            .unwrap_or(0.0);
        let y0 = y - 8.0;
        let wide = width.max(cap_width);
        let (bx0, x1) = if symbol {
            (x - wide / 2.0, x + wide / 2.0)
        } else {
            (x0, x0 + wide)
        };
        let y1 = if caption.is_some() { y + 22.0 } else { y + 8.0 };
        if !hovered
            && placed
                .iter()
                .any(|p| bx0 < p.2 && x1 > p.0 && y0 < p.3 && y1 > p.1)
        {
            continue;
        }
        placed.push((bx0, y0, x1, y1));
        ctx.set_fill_style_str(if hovered { &s.label_hover } else { &s.label });
        let _ = ctx.fill_text(&n.label, x0 as f64, y as f64);
        if let Some(c) = caption {
            let cx = if symbol { x - cap_width / 2.0 } else { x0 };
            ctx.set_font("11px 'Segoe UI', sans-serif");
            ctx.set_global_alpha(0.7);
            let _ = ctx.fill_text(c, cx as f64, (y + 14.0) as f64);
            ctx.set_global_alpha(1.0);
            ctx.set_font("12px 'Segoe UI', sans-serif");
        }
        drawn += 1;
    }
}

fn install_pointer_handlers(overlay: &web_sys::HtmlCanvasElement, state: Rc<RefCell<State>>) {
    let target: &web_sys::EventTarget = overlay.as_ref();
    let el = overlay.clone();
    let local = move |e: &web_sys::MouseEvent| -> (f32, f32) {
        let r = el.get_bounding_client_rect();
        (
            (e.client_x() as f64 - r.left()) as f32,
            (e.client_y() as f64 - r.top()) as f32,
        )
    };

    // pointer down: start pan or node drag; a second finger starts a pinch
    {
        let st = state.clone();
        let local = local.clone();
        let el = overlay.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
                e.prevent_default();
                let (x, y) = local(&e);
                let mut s = st.borrow_mut();
                if e.button() > 2 {
                    return;
                }
                s.drag_moved = false;
                s.press = (x, y);
                if e.button() == 1 || s.space_pan {
                    s.dragging = Drag::Pan { last: (x, y) };
                    s.auto_fit = false;
                    return;
                }
                if e.pointer_type() == "touch" {
                    // Keep receiving moves after the finger leaves the canvas.
                    let _ = el.set_pointer_capture(e.pointer_id());
                    s.touches.retain(|t| t.0 != e.pointer_id());
                    s.touches.push((e.pointer_id(), x, y));
                    if s.touches.len() >= 2 {
                        let (a, b) = (s.touches[0], s.touches[1]);
                        // A node picked up by the first finger stays where it is.
                        if let Drag::Node { index } = s.dragging {
                            if let Some(n) = s.graph.nodes.get_mut(index) {
                                n.pinned = true;
                            }
                        }
                        s.dragging = Drag::Pinch {
                            last_dist: ((a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt().max(1.0),
                            last_angle: (b.2 - a.2).atan2(b.1 - a.1),
                            last_mid: ((a.1 + b.1) / 2.0, (a.2 + b.2) / 2.0),
                        };
                        end_wire(&mut s);
                        return;
                    }
                }
                // Edit mode: a port grab starts a wire (spec 031 §5,
                // stage 2) — ports are checked before nodes.
                if s.interaction == Interaction::Edit {
                    let fr = frame::build_frame(&s.graph, s.camera.three_d);
                    if let Some(p) = frame::port_at(&fr, &s.camera, x, y) {
                        let port = &s.graph.ports[p];
                        let key = (s.graph.nodes[port.node].id.clone(), port.name.clone());
                        s.dragging = Drag::Wire {
                            node: key.0.clone(),
                            name: key.1.clone(),
                        };
                        s.auto_fit = false;
                        s.wire_from = Some(key);
                        s.wire_to = None;
                        s.pos_rev += 1; // the pending wire appears…
                        s.attr_rev += 1; // …and its style with it
                        s.dirty = true;
                        return;
                    }
                }
                if s.camera.three_d {
                    if e.button() == 2 || e.shift_key() {
                        s.dragging = Drag::Orbit { last: (x, y) };
                        return;
                    }
                    // A node under the pointer drags in its own depth plane (Milestone 9).
                    if let Some(i) = s.camera.hit(&s.graph, x, y).filter(|i| {
                        node_visible(&s.graph, *i) && s.camera.node_draggable(&s.graph, *i)
                    }) {
                        s.graph.nodes[i].pinned = true;
                        s.dragging = Drag::Node { index: i };
                    } else {
                        s.dragging = Drag::Pan { last: (x, y) };
                    }
                    return;
                }
                let hit = s.camera.hit(&s.graph, x, y).filter(|i| {
                    node_visible(&s.graph, *i) && s.camera.node_draggable(&s.graph, *i)
                });
                s.dragging = match hit {
                    Some(i) => {
                        s.graph.nodes[i].pinned = true;
                        Drag::Node { index: i }
                    }
                    None => Drag::Pan { last: (x, y) },
                };
            });
        let _ = target.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // pointer move: pan / drag node / hover / pinch
    {
        let st = state.clone();
        let local = local.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(
            move |e: web_sys::PointerEvent| {
                let (x, y) = local(&e);
                let mut s = st.borrow_mut();
                s.pointer = (x, y);
                if e.pointer_type() == "touch" {
                    if let Some(t) = s.touches.iter_mut().find(|t| t.0 == e.pointer_id()) {
                        t.1 = x;
                        t.2 = y;
                    }
                }
                if let Drag::Pinch {
                    last_dist,
                    last_angle,
                    last_mid,
                } = s.dragging
                {
                    if s.touches.len() < 2 {
                        return;
                    }
                    let (a, b) = (s.touches[0], s.touches[1]);
                    let dist = ((a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt().max(1.0);
                    let angle = (b.2 - a.2).atan2(b.1 - a.1);
                    let mid = ((a.1 + b.1) / 2.0, (a.2 + b.2) / 2.0);
                    s.auto_fit = false;
                    s.camera.zoom_at(dist / last_dist, mid.0, mid.1);
                    s.camera.pan(mid.0 - last_mid.0, mid.1 - last_mid.1);
                    if s.camera.three_d {
                        // Fingers turning = the camera turning around its target.
                        let mut da = angle - last_angle;
                        if da > std::f32::consts::PI {
                            da -= 2.0 * std::f32::consts::PI;
                        } else if da < -std::f32::consts::PI {
                            da += 2.0 * std::f32::consts::PI;
                        }
                        s.camera.yaw -= da;
                    }
                    s.dragging = Drag::Pinch {
                        last_dist: dist,
                        last_angle: angle,
                        last_mid: mid,
                    };
                    s.drag_moved = true; // two fingers are never a tap
                    s.dirty = true;
                    return;
                }
                // A release within ~4 px of the press is a tap, not a drag
                // — a touch tap jitters this much (B8 of the review).
                let far = {
                    let (dx, dy) = (x - s.press.0, y - s.press.1);
                    dx * dx + dy * dy > 16.0
                };
                let drag = s.dragging.clone();
                match drag {
                    Drag::Pan { last } => {
                        s.auto_fit = false;
                        s.camera.pan(x - last.0, y - last.1);
                        s.dragging = Drag::Pan { last: (x, y) };
                        s.drag_moved |= far;
                        s.dirty = true;
                    }
                    Drag::Orbit { last } => {
                        s.auto_fit = false;
                        s.camera.orbit(x - last.0, y - last.1);
                        s.dragging = Drag::Orbit { last: (x, y) };
                        s.drag_moved |= far;
                        s.dirty = true;
                    }
                    Drag::Node { index } if index >= s.graph.nodes.len() => {
                        s.dragging = Drag::None;
                    }
                    Drag::Node { index } => {
                        s.auto_fit = false;
                        if s.camera.three_d {
                            let n = &s.graph.nodes[index];
                            if let Some((_, _, w)) = s.camera.project(n.x, n.y, n.z) {
                                let (wx, wy, wz) = s.camera.unproject(x, y, w);
                                let n = &mut s.graph.nodes[index];
                                n.x = wx;
                                n.y = wy;
                                n.z = wz;
                            }
                            s.drag_moved |= far;
                            s.pos_rev += 1;
                            s.dirty = true;
                            return;
                        }
                        let (wx, wy) = s.camera.screen_to_world(x, y);
                        let grid = if e.alt_key() { 0.0 } else { s.graph.grid };
                        s.graph.nodes[index].x = crate::graph::snap(wx, grid);
                        s.graph.nodes[index].y = crate::graph::snap(wy, grid);
                        if !s.layout.running {
                            s.layout.temperature = 2.0;
                            s.layout.running = true;
                        }
                        s.drag_moved |= far;
                        s.pos_rev += 1;
                        s.dirty = true;
                    }
                    Drag::Wire { node, name } => {
                        s.auto_fit = false;
                        // Resolved by id on every move: a set_graph under the
                        // gesture ends it instead of indexing a stale port
                        // (B3 of the review).
                        let Some(p) = port_index(&s.graph, &node, &name) else {
                            s.dragging = Drag::None;
                            end_wire(&mut s);
                            return;
                        };
                        // The wire follows the pointer at its source port's
                        // depth (the same plane rule as a dragged node).
                        let from = frame::port_world(&s.graph, &s.graph.ports[p]);
                        let end = if s.camera.three_d {
                            s.camera
                                .project(from[0], from[1], from[2])
                                .map(|(_, _, w)| {
                                    let (wx, wy, wz) = s.camera.unproject(x, y, w);
                                    [wx, wy, wz]
                                })
                        } else {
                            let (wx, wy) = s.camera.screen_to_world(x, y);
                            Some([wx, wy, from[2]])
                        };
                        if let Some(end) = end {
                            s.wire_to = Some(end);
                            s.drag_moved |= far;
                            s.pos_rev += 1;
                            s.dirty = true;
                        }
                    }
                    Drag::Pinch { .. } => {}
                    Drag::None => {
                        let hit = s
                            .camera
                            .hit(&s.graph, x, y)
                            .filter(|i| node_visible(&s.graph, *i));
                        if hit != s.hovered {
                            s.hovered = hit;
                            s.hover_rev += 1;
                            s.dirty = true;
                            let payload = match hit {
                                Some(i) => {
                                    let n = &s.graph.nodes[i];
                                    serde_json::json!({ "kind": "hover", "id": n.id, "label": n.label, "nodeKind": n.kind, "key": n.key, "x": x, "y": y })
                                }
                                None => serde_json::json!({ "kind": "hover", "id": null }),
                            };
                            emit(&s, payload);
                        }
                        // An edge's label names the stroke under the
                        // pointer (spec 031 §2, stage 9). A node hover
                        // wins; the scan is one pass over every drawn
                        // segment, so it waits for a labelled graph.
                        let eh = if hit.is_none() && s.has_edge_labels {
                            frame::edge_at(&s.renderer.frame(), &s.camera, x, y)
                        } else {
                            None
                        };
                        if eh != s.hovered_edge {
                            s.hovered_edge = eh;
                            s.dirty = true;
                        }
                    }
                }
            },
        );
        let _ = target.add_event_listener_with_callback("pointermove", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // pointer up: end drag; click / double-click detection
    {
        let st = state.clone();
        let local = local.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(
            move |e: web_sys::PointerEvent| {
                let (x, y) = local(&e);
                let mut s = st.borrow_mut();
                if e.pointer_type() == "touch" {
                    s.touches.retain(|t| t.0 != e.pointer_id());
                    if matches!(s.dragging, Drag::Pinch { .. }) {
                        // One finger left: it pans from where it is; no click.
                        s.dragging = match s.touches.first() {
                            Some(t) => Drag::Pan { last: (t.1, t.2) },
                            None => Drag::None,
                        };
                        return;
                    }
                }
                // A released wire completes or cancels (spec 031 §5):
                // dropped on another port it emits `connect` — the host
                // validates types and adds the edge; the renderer never
                // mutates topology.
                if let Drag::Wire { node, name } = s.dragging.clone() {
                    // The wire's segment and its style leave the buffers
                    // together (B4): `end_wire` bumps both revisions.
                    s.dragging = Drag::None;
                    s.drag_moved = true; // a wire is a gesture, not a pick
                    let fr = frame::build_frame(&s.graph, s.camera.three_d);
                    let over = frame::port_at(&fr, &s.camera, x, y);
                    let from = port_index(&s.graph, &node, &name);
                    let to = over.and_then(|p| {
                        let port = &s.graph.ports[p];
                        (port.name != name || s.graph.nodes[port.node].id != node).then_some(p)
                    });
                    if let (Some(from), Some(to)) = (from, to) {
                        let fp = &s.graph.ports[from];
                        let tp = &s.graph.ports[to];
                        let payload = serde_json::json!({
                            "kind": "connect",
                            "from": { "node": s.graph.nodes[fp.node].id, "port": fp.name },
                            "to": { "node": s.graph.nodes[tp.node].id, "port": tp.name },
                        });
                        s.last_connect = Some(payload.to_string());
                        emit(&s, payload);
                    }
                    end_wire(&mut s);
                    return;
                }
                // A node the user placed stays put (pinned) until Relayout.
                let was_node = matches!(s.dragging, Drag::Node { .. });
                let drag_id = if let Drag::Node { index } = s.dragging {
                    s.graph.nodes.get(index).map(|n| n.id.clone())
                } else {
                    None
                };
                s.dragging = Drag::None;
                let moved = s.drag_moved;
                s.drag_moved = false;
                if moved {
                    if was_node {
                        // A block finished moving (spec 031 §5, stage 7):
                        // the host harvests the position into its document.
                        // The payload is built here, under the borrow —
                        // the host cannot call back into the view from
                        // inside an event (the RefCell is held).
                        if let Some(n) =
                            drag_id.and_then(|id| s.graph.nodes.iter().find(|n| n.id == id))
                        {
                            let screen = if s.camera.three_d {
                                s.camera.project(n.x, n.y, n.z).map(|(x, y, _)| vec![x, y])
                            } else {
                                let (x, y) = s.camera.world_to_screen(n.x, n.y);
                                Some(vec![x, y])
                            };
                            let mut positions = serde_json::Map::new();
                            positions.insert(n.id.clone(), serde_json::json!([n.x, n.y]));
                            emit(
                                &s,
                                serde_json::json!({
                                    "kind": "moved",
                                    "positions": positions,
                                    "unsnapped": e.alt_key(),
                                    "screen": screen,
                                }),
                            );
                        }
                    }
                    return; // a released pan or node move is not a pick
                }
                if s.space_pan {
                    return;
                }
                if e.button() == 1 {
                    if let Some(i) = s
                        .camera
                        .hit(&s.graph, x, y)
                        .filter(|i| node_visible(&s.graph, *i))
                    {
                        emit(
                            &s,
                            serde_json::json!({"kind":"openBeside", "id":s.graph.nodes[i].id}),
                        );
                    }
                    return;
                }
                if e.button() != 0 {
                    return;
                }
                if let Some(i) = s
                    .camera
                    .hit(&s.graph, x, y)
                    .filter(|i| node_visible(&s.graph, *i))
                {
                    let now = js_sys::Date::now();
                    let dbl = now - s.last_click_ms < 350.0;
                    s.last_click_ms = if dbl { 0.0 } else { now };
                    // Selecting (spec 031 §5): a plain click replaces the
                    // selection, Ctrl/Meta toggles one instance.
                    let id = s.graph.nodes[i].id.clone();
                    let toggle = e.ctrl_key() || e.meta_key();
                    let before = s.selected.clone();
                    if toggle {
                        if !s.selected.nodes.remove(&id) {
                            s.selected.nodes.insert(id.clone());
                        }
                    } else {
                        s.selected = frame::Selection {
                            nodes: [id.clone()].into_iter().collect(),
                            edges: Default::default(),
                        };
                    }
                    if before != s.selected {
                        s.sel_rev += 1;
                        emit_select(&mut s);
                    }
                    emit(
                        &s,
                        serde_json::json!({ "kind": if dbl { "dblclick" } else { "click" }, "id": id }),
                    );
                    return;
                }
                // No node under the pointer: an edge, or empty space.
                let fr = frame::build_frame(&s.graph, s.camera.three_d);
                match frame::edge_at(&fr, &s.camera, x, y) {
                    Some(i) => {
                        let id = s.graph.edges[i].id.clone();
                        let toggle = e.ctrl_key() || e.meta_key();
                        let before = s.selected.clone();
                        if toggle {
                            if !s.selected.edges.remove(&id) {
                                s.selected.edges.insert(id);
                            }
                        } else {
                            s.selected = frame::Selection {
                                nodes: Default::default(),
                                edges: [id].into_iter().collect(),
                            };
                        }
                        if before != s.selected {
                            s.sel_rev += 1;
                            emit_select(&mut s);
                        }
                    }
                    None => {
                        if !s.selected.is_empty() {
                            s.selected = frame::Selection::default();
                            s.sel_rev += 1;
                            emit_select(&mut s);
                        }
                    }
                }
            },
        );
        let _ = target.add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // pointer cancel (the system took the touch): forget the finger
    {
        let st = state.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
                let mut s = st.borrow_mut();
                s.touches.retain(|t| t.0 != e.pointer_id());
                if s.touches.len() < 2 {
                    s.dragging = Drag::None;
                    end_wire(&mut s);
                }
            });
        let _ =
            target.add_event_listener_with_callback("pointercancel", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // leave: clear hover
    {
        let st = state.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
                let mut s = st.borrow_mut();
                if e.pointer_type() == "touch" {
                    return; // captured pointers report leave while still down
                }
                s.dragging = Drag::None;
                end_wire(&mut s);
                if s.hovered.take().is_some() {
                    s.hover_rev += 1;
                    s.dirty = true;
                    emit(&s, serde_json::json!({ "kind": "hover", "id": null }));
                }
            });
        let _ =
            target.add_event_listener_with_callback("pointerleave", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // right button orbits in 3D: no context menu on the canvas
    {
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            e.prevent_default();
        });
        let _ = target.add_event_listener_with_callback("contextmenu", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // wheel: zoom at pointer
    {
        let st = state.clone();
        let el = overlay.clone();
        let cb = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |e: web_sys::WheelEvent| {
            e.prevent_default();
            let r = el.get_bounding_client_rect();
            let (x, y) = (
                (e.client_x() as f64 - r.left()) as f32,
                (e.client_y() as f64 - r.top()) as f32,
            );
            let factor = if e.delta_y() < 0.0 { 1.12 } else { 1.0 / 1.12 };
            let mut s = st.borrow_mut();
            s.auto_fit = false;
            s.camera.zoom_at(factor, x, y);
            s.dirty = true;
        });
        let _ = target.add_event_listener_with_callback("wheel", cb.as_ref().unchecked_ref());
        cb.forget();
    }
}
