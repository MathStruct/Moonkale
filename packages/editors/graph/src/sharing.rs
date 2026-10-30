//! A pointer-only adapter: viewport geometry in, world coordinates out.
use dioxus::prelude::*;
use moonkale_ext_api::{sharing::Pointer, Workspace};

#[component]
pub fn SharedPointers(mut ws: Workspace, id: String, view_key: String) -> Element {
    let mut bridge = use_signal(|| None::<document::Eval>);
    let mount_id = id.clone();
    let mut current = use_signal(|| view_key.clone());
    if *current.peek() != view_key {
        ws.share_pointer(None);
        current.set(view_key.clone());
    }
    let enabled = ws.session.sharing.read().pointer;
    use_effect(use_reactive!(|enabled| {
        if let Some(eval) = bridge.read().as_ref() {
            let _ = eval.send(enabled);
        }
    }));
    let _ = ws.session.presence.read();
    let pointers: Vec<_> = ws
        .others()
        .into_iter()
        .filter_map(|member| {
            let p = member.sharing.as_ref()?.pointer.as_ref()?;
            (p.view == view_key).then(|| (member.clone(), p.clone()))
        })
        .collect();
    use_drop(move || {
        ws.share_pointer(None);
        if let Some(e) = bridge.peek().as_ref() {
            let _ = e.send(serde_json::Value::Null);
        }
    });
    rsx! {
        div { class: "mk-shared-pointers", id: "{id}-pointers", aria_hidden: "true",
            onmounted: move |_| {
                let script = SCRIPT.replace("GRAPH_ID", &serde_json::to_string(&mount_id).unwrap());
                let mut e = document::eval(&script);
                let _ = e.send(ws.session.sharing.peek().pointer);
                bridge.set(Some(e));
                spawn(async move {
                    while let Ok(point) = e.recv::<Option<[f64;2]>>().await {
                        ws.share_pointer(point.map(|[x,y]| Pointer { view: current.peek().clone(), x, y }));
                    }
                });
            },
            for (member, p) in pointers {
                span { key: "{member.window}", class: "mk-shared-pointer", "data-world-x": p.x.to_string(), "data-world-y": p.y.to_string(), "data-color": member.color().to_string(), "data-participant": member.name.clone(),
                    span { class: "mk-shared-arrow", "➤" }
                    span { class: "mk-shared-pointer-name", "{member.name}" }
                }
            }
        }
    }
}
const SCRIPT: &str = r#"
const id = GRAPH_ID;
const overlay = document.getElementById(id + '-ov');
const layer = document.getElementById(id + '-pointers');
if (!overlay || !layer) return;
let enabled = false, live = true, frame, latest, last = 0;
const move = e => { if (enabled) latest = [e.clientX,e.clientY]; };
const leave = () => { latest = null; if (enabled) dioxus.send(null); };
overlay.addEventListener('pointermove', move);
overlay.addEventListener('pointerleave', leave);
function draw(now) {
  if (!live) return;
  const view = window.moonkale?.graphViews?.[id];
  if (view && view.mode() === '2d') {
    const [scale,cx,cy] = view.camera_state();
    const rect = overlay.getBoundingClientRect();
    // camera_state uses CSS dimensions, independent of devicePixelRatio.
    for (const node of layer.children) {
      const x = (Number(node.dataset.worldX)-cx)*scale+rect.width/2;
      const y = (Number(node.dataset.worldY)-cy)*scale+rect.height/2;
      node.style.transform = `translate(${x}px,${y}px)`;
      node.style.visibility = 'visible';
    }
    if (enabled && latest && now-last >= 50) {
      last=now;
      dioxus.send([(latest[0]-rect.left-rect.width/2)/scale+cx,(latest[1]-rect.top-rect.height/2)/scale+cy]);
      latest = null;
    }
  } else { for (const node of layer.children) node.style.visibility = 'hidden'; }
  frame=requestAnimationFrame(draw);
}
frame=requestAnimationFrame(draw);
for (;;) {
  const value=await dioxus.recv();
  if (value === null) break;
  enabled=value;
  if (!enabled) latest=null;
}
live=false; cancelAnimationFrame(frame);
overlay.removeEventListener('pointermove',move); overlay.removeEventListener('pointerleave',leave);
"#;
