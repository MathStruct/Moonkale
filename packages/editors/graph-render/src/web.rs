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
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Pan { last: (f32, f32) },
    Node { index: usize },
}

#[wasm_bindgen]
pub struct GraphView {
    state: Rc<RefCell<State>>,
}

fn emit(state: &State, value: serde_json::Value) {
    let js = js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL);
    let _ = state.on_event.call1(&JsValue::NULL, &js);
}

#[wasm_bindgen]
pub async fn create(
    canvas: web_sys::HtmlCanvasElement,
    overlay: web_sys::HtmlCanvasElement,
    on_event: js_sys::Function,
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
    let renderer = Renderer::new(canvas.clone(), (w * dpr) as u32, (h * dpr) as u32)
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
    pub fn set_graph(&self, json: &str) -> Result<(), JsValue> {
        let input: InGraph =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut s = self.state.borrow_mut();
        s.graph = Graph::from_input(input);
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
        s.dirty = true;
    }

    pub fn backend(&self) -> String {
        self.state.borrow().renderer.backend.clone()
    }

    pub fn node_count(&self) -> usize {
        self.state.borrow().graph.nodes.len()
    }

    /// Screen position of a node by id (for tests and for the host's popup).
    pub fn node_screen_position(&self, id: &str) -> Option<Vec<f32>> {
        let s = self.state.borrow();
        let i = s.graph.nodes.iter().position(|n| n.id == id)?;
        let (x, y) = s
            .camera
            .world_to_screen(s.graph.nodes[i].x, s.graph.nodes[i].y);
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
    // Labels only when there's room: zoomed in, or few nodes.
    let show_all = scale >= 0.9 || s.graph.nodes.len() <= 60;
    ctx.set_font("12px 'Segoe UI', sans-serif");
    ctx.set_text_baseline("middle");
    let mut drawn = 0;
    for (i, n) in s.graph.nodes.iter().enumerate() {
        let hovered = Some(i) == s.hovered;
        if !show_all && !hovered && n.degree < 3 {
            continue;
        }
        let (x, y) = s.camera.world_to_screen(n.x, n.y);
        if x < -100.0 || y < -20.0 || x > s.camera.width + 100.0 || y > s.camera.height + 20.0 {
            continue;
        }
        if drawn > 400 && !hovered {
            break;
        }
        let r = n.radius * scale.max(0.6);
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

    // pointer down: start pan or node drag
    {
        let st = state.clone();
        let local = local.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            e.prevent_default();
            let (x, y) = local(&e);
            let mut s = st.borrow_mut();
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
    // pointer move: pan / drag node / hover
    {
        let st = state.clone();
        let local = local.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let (x, y) = local(&e);
            let mut s = st.borrow_mut();
            match s.dragging {
                Drag::Pan { last } => {
                    s.auto_fit = false;
                    s.camera.pan(x - last.0, y - last.1);
                    s.dragging = Drag::Pan { last: (x, y) };
                    s.dirty = true;
                }
                Drag::Node { index } => {
                    s.auto_fit = false;
                    let (wx, wy) = s.camera.screen_to_world(x, y);
                    s.graph.nodes[index].x = wx;
                    s.graph.nodes[index].y = wy;
                    if !s.layout.running {
                        s.layout.temperature = 2.0;
                        s.layout.running = true;
                    }
                    s.dirty = true;
                }
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
        });
        let _ = target.add_event_listener_with_callback("pointermove", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // pointer up: end drag; click / double-click detection
    {
        let st = state.clone();
        let local = local.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let (x, y) = local(&e);
            let mut s = st.borrow_mut();
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
        });
        let _ = target.add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref());
        cb.forget();
    }
    // leave: clear hover
    {
        let st = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_e: web_sys::MouseEvent| {
            let mut s = st.borrow_mut();
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
