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
use crate::graph::{Graph, InGraph};
use crate::layout::Layout;
use crate::render::Renderer;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

struct State {
    graph: Graph,
    layout: Layout,
    camera: Camera,
    renderer: Renderer,
    overlay: web_sys::CanvasRenderingContext2d,
    dpr: f32,
    hovered: Option<usize>,
    dragging: Drag,
    on_event: js_sys::Function,
    dirty: bool,
    alive: bool,
    last_click_ms: f64,
    /// Fit the view when the layout settles — only until the user has moved
    /// the camera or a node; after that, their view is theirs.
    auto_fit: bool,
    /// Active touch pointers `(id, x, y)`, for two-finger gestures.
    touches: Vec<(i32, f32, f32)>,
}

#[derive(Clone, Copy, PartialEq)]
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
}

#[wasm_bindgen]
pub struct GraphView {
    state: Rc<RefCell<State>>,
}

fn emit(state: &State, value: serde_json::Value) {
    let js = js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL);
    let _ = state.on_event.call1(&JsValue::NULL, &js);
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
        })
        .collect();
    let mut edges = Vec::new();
    for i in 1..n {
        edges.push(InEdge {
            a: i,
            b: i / 7,
            kind: "contains".into(),
            color: None,
        });
        if i % 5 == 0 {
            edges.push(InEdge {
                a: i,
                b: (i * 7919) % n,
                kind: "links".into(),
                color: None,
            });
        }
    }
    let mut g = crate::graph::Graph::from_input(InGraph { nodes, edges });
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
    let layout = Layout::new(&graph);
    let state = Rc::new(RefCell::new(State {
        camera: Camera {
            width: w,
            height: h,
            ..Default::default()
        },
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
        last_click_ms: 0.0,
        auto_fit: true,
        touches: Vec::new(),
    }));
    let backend = state.borrow().renderer.backend.clone();
    emit(
        &state.borrow(),
        serde_json::json!({ "kind": "ready", "backend": backend }),
    );
    install_pointer_handlers(&overlay, state.clone());
    start_loop(state.clone());
    Ok(GraphView { state })
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
        let input: InGraph =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut s = self.state.borrow_mut();
        let mut next = Graph::from_input(input);
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
        let incremental = !old.is_empty() && shared * 2 >= next.nodes.len().max(1);
        if incremental {
            // Known nodes stay put; new ones start at the mean of their known
            // neighbours (or where the spiral put them) with a nudge so two
            // new siblings do not coincide.
            let known: Vec<bool> = next.nodes.iter().map(|n| old.contains_key(&n.id)).collect();
            let mut placed: Vec<Option<(f32, f32, f32)>> = vec![None; next.nodes.len()];
            for (i, n) in next.nodes.iter().enumerate() {
                if let Some(&(x, y, z, _)) = old.get(&n.id) {
                    placed[i] = Some((x, y, z));
                }
            }
            for (i, n) in next.nodes.iter().enumerate() {
                if known[i] {
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
            s.graph = next;
            if changed || !same_edges {
                // Warm, not hot: known nodes drift a little, new ones settle in.
                let mut l = Layout::new(&s.graph);
                l.temperature = 3.0;
                s.layout = l;
            }
            s.hovered = None;
            s.dirty = true;
            return Ok(());
        }
        s.graph = next;
        s.layout = Layout::new(&s.graph);
        s.hovered = None;
        s.auto_fit = true;
        // A rough fit up front so the first frames are on screen; fit again when settled.
        let g = std::mem::take(&mut s.graph);
        s.camera.fit(&g, 40.0);
        s.graph = g;
        s.dirty = true;
        Ok(())
    }

    pub fn fit(&self) {
        let mut s = self.state.borrow_mut();
        let g = std::mem::take(&mut s.graph);
        s.camera.fit(&g, 40.0);
        s.graph = g;
        s.dirty = true;
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

    /// `"2d"` or `"3d"` (Milestone 8): the same graph, a perspective camera
    /// orbiting the layout with one plane per node kind.
    pub fn set_mode(&self, mode: &str) {
        let mut s = self.state.borrow_mut();
        let three_d = mode == "3d";
        if s.camera.three_d != three_d {
            s.camera.three_d = three_d;
            s.hovered = None;
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

    /// `[scale, cx, cy, yaw, pitch, dist]` — the camera, for tests (spec 006).
    pub fn camera_state(&self) -> Vec<f32> {
        let c = &self.state.borrow().camera;
        vec![c.scale, c.cx, c.cy, c.yaw, c.pitch, c.dist]
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

fn start_loop(state: Rc<RefCell<State>>) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    let st = state.clone();
    *g.borrow_mut() = Some(Closure::new(move || {
        let mut s = st.borrow_mut();
        if !s.alive {
            return;
        }
        // Layout: more iterations per frame for small graphs.
        if s.layout.running {
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
            s.dirty = true;
            if !s.layout.running {
                if s.auto_fit {
                    let g = std::mem::take(&mut s.graph);
                    s.camera.fit(&g, 40.0);
                    s.graph = g;
                }
                emit(&s, serde_json::json!({ "kind": "settled" }));
            }
        }
        if s.dirty {
            s.dirty = false;
            let State {
                graph,
                camera,
                renderer,
                hovered,
                ..
            } = &mut *s;
            renderer.draw(graph, camera, *hovered);
            draw_labels(&s);
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
    // Labels only when there's room: zoomed in, or few nodes. Past 20k nodes
    // the overlay scan itself costs frames: hovered label only.
    let huge = s.graph.nodes.len() > 20_000;
    let show_all = !huge && ((!s.camera.three_d && scale >= 0.9) || s.graph.nodes.len() <= 60);
    ctx.set_font("12px 'Segoe UI', sans-serif");
    ctx.set_text_baseline("middle");
    let mut drawn = 0;
    for (i, n) in s.graph.nodes.iter().enumerate() {
        let hovered = Some(i) == s.hovered;
        if !show_all && !hovered && (huge || n.degree < 3) {
            continue;
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
        if drawn > 400 && !hovered {
            break;
        }
        ctx.set_fill_style_str(if hovered {
            "#ffffff"
        } else {
            "rgba(230,232,238,0.85)"
        });
        let _ = ctx.fill_text(&n.label, (x + r + 4.0) as f64, y as f64);
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
                if e.pointer_type() == "touch" {
                    // Keep receiving moves after the finger leaves the canvas.
                    let _ = el.set_pointer_capture(e.pointer_id());
                    s.touches.retain(|t| t.0 != e.pointer_id());
                    s.touches.push((e.pointer_id(), x, y));
                    if s.touches.len() >= 2 {
                        let (a, b) = (s.touches[0], s.touches[1]);
                        // A node picked up by the first finger stays where it is.
                        if let Drag::Node { index } = s.dragging {
                            s.graph.nodes[index].pinned = true;
                        }
                        s.dragging = Drag::Pinch {
                            last_dist: ((a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt().max(1.0),
                            last_angle: (b.2 - a.2).atan2(b.1 - a.1),
                            last_mid: ((a.1 + b.1) / 2.0, (a.2 + b.2) / 2.0),
                        };
                        return;
                    }
                }
                if s.camera.three_d {
                    if e.button() == 2 || e.shift_key() {
                        s.dragging = Drag::Orbit { last: (x, y) };
                        return;
                    }
                    // A node under the pointer drags in its own depth plane (Milestone 9).
                    if let Some(i) = s.camera.hit(&s.graph, x, y) {
                        s.graph.nodes[i].pinned = true;
                        s.dragging = Drag::Node { index: i };
                    } else {
                        s.dragging = Drag::Pan { last: (x, y) };
                    }
                    return;
                }
                let hit = s.camera.hit(&s.graph, x, y);
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
                    s.dirty = true;
                    return;
                }
                match s.dragging {
                    Drag::Pan { last } => {
                        s.auto_fit = false;
                        s.camera.pan(x - last.0, y - last.1);
                        s.dragging = Drag::Pan { last: (x, y) };
                        s.dirty = true;
                    }
                    Drag::Orbit { last } => {
                        s.auto_fit = false;
                        s.camera.orbit(x - last.0, y - last.1);
                        s.dragging = Drag::Orbit { last: (x, y) };
                        s.dirty = true;
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
                            s.dirty = true;
                            return;
                        }
                        let (wx, wy) = s.camera.screen_to_world(x, y);
                        s.graph.nodes[index].x = wx;
                        s.graph.nodes[index].y = wy;
                        if !s.layout.running {
                            s.layout.temperature = 2.0;
                            s.layout.running = true;
                        }
                        s.dirty = true;
                    }
                    Drag::Pinch { .. } => {}
                    Drag::None => {
                        let hit = s.camera.hit(&s.graph, x, y);
                        if hit != s.hovered {
                            s.hovered = hit;
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
                // A node the user placed stays put (pinned) until Relayout.
                s.dragging = Drag::None;
                if let Some(i) = s.camera.hit(&s.graph, x, y) {
                    let now = js_sys::Date::now();
                    let dbl = now - s.last_click_ms < 350.0;
                    s.last_click_ms = if dbl { 0.0 } else { now };
                    let id = s.graph.nodes[i].id.clone();
                    emit(
                        &s,
                        serde_json::json!({ "kind": if dbl { "dblclick" } else { "click" }, "id": id }),
                    );
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
                if s.hovered.take().is_some() {
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
