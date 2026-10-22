//! Shared 2D canvas components. The host supplies camera events, world bounds,
//! children, and a drag callback; this crate knows no editor or graph model.
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use serde::Deserialize;

/// Engine camera event in CSS pixels (independent of device pixel ratio).
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasCamera {
    pub scale: f32,
    pub offset: [f32; 2],
    pub width: f32,
    pub height: f32,
    pub three_d: bool,
}

impl CanvasCamera {
    /// LOD uses projected bounds, and rejects invalid transforms and 3D views.
    pub fn visible(&self, bounds: [f32; 4], min_size: [f32; 2]) -> bool {
        let [x, y, w, h] = bounds;
        let [ox, oy] = self.offset;
        if self.three_d
            || self.scale <= 0.0
            || ![x, y, w, h, ox, oy, self.scale, self.width, self.height]
                .iter()
                .all(|n| n.is_finite())
        {
            return false;
        }
        let (x, y, w, h) = (
            x * self.scale + ox,
            y * self.scale + oy,
            w * self.scale,
            h * self.scale,
        );
        w >= min_size[0]
            && h >= min_size[1]
            && x < self.width
            && y < self.height
            && x + w > 0.0
            && y + h > 0.0
    }
}

/// Exactly one transformed layer per canvas, above the GPU and label canvases.
#[component]
pub fn CanvasLayer(camera: CanvasCamera, children: Element) -> Element {
    rsx! {
        div { class: "mk-canvas-clip", style: "position:absolute;inset:0;overflow:hidden;pointer-events:none;z-index:5;",
            div { class: "mk-canvas-layer",
                style: "position:absolute;left:0;top:0;transform-origin:0 0;pointer-events:none;transform:translate({camera.offset[0]}px,{camera.offset[1]}px) scale({camera.scale});",
                {children}
            }
        }
    }
}

/// A world-space card. Children are absent from the DOM below LOD/off screen.
#[component]
pub fn CanvasItem(
    camera: CanvasCamera,
    bounds: [f32; 4],
    #[props(default = [100.0, 32.0])] min_size: [f32; 2],
    children: Element,
) -> Element {
    if !camera.visible(bounds, min_size) {
        return rsx! {};
    }
    rsx! {
        div { class: "mk-canvas-item",
            style: "position:absolute;left:{bounds[0]}px;top:{bounds[1]}px;width:{bounds[2]}px;pointer-events:auto;",
            onpointerdown: move |e| e.stop_propagation(),
            onclick: move |e| e.stop_propagation(),
            ondoubleclick: move |e| e.stop_propagation(),
            onwheel: move |e| e.stop_propagation(),
            {children}
        }
    }
}

/// World-space motion of a card header. The host owns persistence and undo.
#[derive(Clone, Copy, Debug)]
pub struct CardDrag {
    pub delta: [f32; 2],
    pub finished: bool,
    pub cancelled: bool,
}

/// Pointer capture is DOM plumbing only; drag coordinates/state live in Rust.
#[component]
pub fn CanvasHeader(
    id: String,
    scale: f32,
    ondrag: EventHandler<CardDrag>,
    children: Element,
) -> Element {
    let mut press = use_signal(|| None::<(i32, f64, f64, f32)>);
    let capture_id = id.clone();
    rsx! {
        div { id, class: "mk-canvas-header", style: "cursor:grab;touch-action:none;",
            onpointerdown: move |e| {
                if e.trigger_button() != Some(MouseButton::Primary) { return; }
                e.stop_propagation(); e.prevent_default();
                let p = e.client_coordinates();
                press.set(Some((e.pointer_id(), p.x, p.y, scale)));
                // id is serde-escaped by the host-provided Dioxus eval argument.
                let eval = document::eval("const m = await dioxus.recv(); document.getElementById(m.id)?.setPointerCapture(m.pointer);");
                let _ = eval.send(serde_capture(&capture_id, e.pointer_id()));
                spawn(async move { let _ = eval.await; });
                ondrag.call(CardDrag { delta: [0.0; 2], finished: false, cancelled: false });
            },
            onpointermove: move |e| {
                let start = *press.peek();
                if let Some((pointer,x,y,s)) = start.filter(|p| p.0 == e.pointer_id()) {
                    let _ = pointer;
                    e.stop_propagation();
                    let p = e.client_coordinates();
                    ondrag.call(CardDrag { delta: [((p.x-x)/s as f64) as f32, ((p.y-y)/s as f64) as f32], finished: false, cancelled: false });
                }
            },
            onpointerup: move |e| {
                let start = *press.peek();
                if let Some((_,x,y,s)) = start.filter(|p| p.0 == e.pointer_id()) {
                    press.set(None); e.stop_propagation();
                    let p = e.client_coordinates();
                    ondrag.call(CardDrag { delta: [((p.x-x)/s as f64) as f32, ((p.y-y)/s as f64) as f32], finished: true, cancelled: false });
                }
            },
            onpointercancel: move |_| {
                if press.peek().is_some() { press.set(None); ondrag.call(CardDrag { delta:[0.0;2], finished:true, cancelled:true }); }
            },
            {children}
        }
    }
}

#[derive(serde::Serialize)]
struct Capture<'a> {
    id: &'a str,
    pointer: i32,
}
fn serde_capture(id: &str, pointer: i32) -> Capture<'_> {
    Capture { id, pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projected_size_viewport_and_3d_control_mounting() {
        let mut c = CanvasCamera {
            scale: 1.0,
            width: 800.0,
            height: 600.0,
            ..Default::default()
        };
        let bounds = [10.0, 20.0, 220.0, 100.0];
        assert!(c.visible(bounds, [100.0, 32.0]));
        c.scale = 0.1;
        assert!(!c.visible(bounds, [100.0, 32.0]));
        c.scale = 1.0;
        c.offset = [-240.0, 0.0];
        assert!(!c.visible(bounds, [100.0, 32.0]));
        c.offset = [-200.0, 0.0];
        assert!(c.visible(bounds, [100.0, 32.0]));
        c.three_d = true;
        assert!(!c.visible(bounds, [100.0, 32.0]));
        c.three_d = false;
        c.scale = f32::NAN;
        assert!(!c.visible(bounds, [100.0, 32.0]));
    }
}
