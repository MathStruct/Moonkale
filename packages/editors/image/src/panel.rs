//! The viewer: toolbar (name, dimensions, size, zoom controls) over a
//! scrollable stage with the image at the chosen scale. Zoom keeps the
//! pointer's image point under the pointer on wheel; dragging pans.

use crate::L;
use crate::{mime_of, MAX_BYTES};
use base64::Engine;
use dioxus::prelude::*;
use moonkale_core::Node;
use moonkale_ext_api::{t, Workspace};

const CSS: Asset = asset!("/assets/image.css");
const STEPS: &[f64] = &[0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

#[derive(Clone, PartialEq)]
enum Loaded {
    Loading,
    Ready { url: String, bytes: u64 },
    Failed(String),
}

#[component]
pub fn ImagePanel(ws: Workspace, node: Node) -> Element {
    let mut state: Signal<Loaded> = use_signal(|| Loaded::Loading);
    // `None` = fit to the stage; `Some(scale)` = explicit zoom.
    let mut zoom: Signal<Option<f64>> = use_signal(|| None);
    let mut natural: Signal<Option<(u32, u32)>> = use_signal(|| None);
    let mut drag: Signal<Option<(f64, f64)>> = use_signal(|| None);
    let stage_id = format!("mk-image-stage-{}", node.id);

    // Fetch once per node (the panel remounts for another node).
    {
        let node = node.clone();
        moonkale_ext_api::crash::use_effect(move || {
            let node = node.clone();
            moonkale_ext_api::crash::spawn(async move {
                let too_big = match &node.content {
                    Some(moonkale_core::ContentRef::Blob { len, .. }) => *len > MAX_BYTES,
                    _ => false,
                };
                if too_big {
                    state.set(Loaded::Failed(t!(
                        ws,
                        L,
                        "image-too-big",
                        name = node.label.clone(),
                        mb = MAX_BYTES / 1024 / 1024
                    )));
                    return;
                }
                match ws.fetch_bytes(&node).await {
                    Ok(bytes)
                        if crate::dimensions(&bytes)
                            .is_some_and(|(w, h)| w as u64 * h as u64 > crate::MAX_PIXELS) =>
                    {
                        let (w, h) = crate::dimensions(&bytes).unwrap_or_default();
                        state.set(Loaded::Failed(t!(
                            ws,
                            L,
                            "image-too-many-pixels",
                            name = node.label.clone(),
                            width = w,
                            height = h
                        )));
                    }
                    Ok(bytes) => {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        state.set(Loaded::Ready {
                            url: format!("data:{};base64,{b64}", mime_of(&node.native_key)),
                            bytes: bytes.len() as u64,
                        });
                    }
                    Err(e) => state.set(Loaded::Failed(e.to_string())),
                }
            });
        });
    }

    let title = node.native_key.clone();
    let dims = natural()
        .map(|(w, h)| format!("{w} × {h}"))
        .unwrap_or_default();
    let size = match state() {
        Loaded::Ready { bytes, .. } => human(bytes),
        _ => String::new(),
    };
    let zoom_label = match zoom() {
        None => t!(ws, L, "image-fit"),
        Some(z) => format!("{}%", (z * 100.0).round()),
    };
    let stage_id_for_move = stage_id.clone();
    let stage_id_for_load = stage_id.clone();
    let img_style = match zoom() {
        None => "max-width: 100%; max-height: 100%; width: auto; height: auto;".to_string(),
        Some(z) => match natural() {
            Some((w, h)) => format!(
                "width: {}px; height: {}px; max-width: none; max-height: none;",
                (w as f64 * z).round(),
                (h as f64 * z).round()
            ),
            None => format!("transform: scale({z}); transform-origin: 0 0;"),
        },
    };
    let mut step = move |dir: i32| {
        let current = zoom().unwrap_or(1.0);
        let next = if dir > 0 {
            STEPS
                .iter()
                .copied()
                .find(|s| *s > current + 1e-9)
                .unwrap_or(current)
        } else {
            STEPS
                .iter()
                .rev()
                .copied()
                .find(|s| *s < current - 1e-9)
                .unwrap_or(current)
        };
        zoom.set(Some(next));
    };

    rsx! {
            moonkale_ext_api::Stylesheet { href: CSS }
            div { class: "mk-image", "data-zoom": "{zoom_label}",
                div { class: "mk-editor-toolbar",
                    span { class: "mk-editor-path", "{title}" }
                    span { class: "mk-editor-spacer" }
                    span { class: "mk-editor-meta", "{dims} · {size}" }
                    span { class: "mk-image-zoom", "{zoom_label}" }
                moonkale_ext_api::icons::ToolbarActions {
                        primary: rsx! { moonkale_ext_api::icons::IconButton {
                        icon: "fit",
                        label: t!(ws, L, "image-fit"),
                        description: t!(ws, L, "image-fit-title"),
                        pressed: Some(zoom().is_none()),
                        onclick: moonkale_ext_api::crash::event(move |_| zoom.set(None)),
                    } },
    moonkale_ext_api::icons::IconButton {
                        icon: "zoom-out",
                        label: t!(ws, L, "image-zoom-out"),
                        onclick: moonkale_ext_api::crash::event(move |_| step(-1)),
                    }

                    moonkale_ext_api::icons::IconButton {
                        icon: "zoom-in",
                        label: t!(ws, L, "image-zoom-in"),
                        onclick: moonkale_ext_api::crash::event(move |_| step(1)),
                    }

                    moonkale_ext_api::icons::IconButton {
                        icon: "actual-size",
                        label: "100%".to_string(),
                        description: t!(ws, L, "image-actual-title"),
                        pressed: Some(zoom() == Some(1.0)),
                        onclick: moonkale_ext_api::crash::event(move |_| zoom.set(Some(1.0))),
                    }
                    if node.native_key.to_ascii_lowercase().ends_with(".svg") {
                        moonkale_ext_api::icons::IconButton {
                            icon: "code",
                            label: t!(ws, L, "image-source"),
                            description: t!(ws, L, "image-svg-title"),
                            onclick: moonkale_ext_api::crash::event({
                                let n = node.clone();
                                move |_| {
                                    let n = n.clone();
                                    moonkale_ext_api::crash::spawn(async move { let _ = ws.open_as_text(n).await; });
                                }
                            }),
                        }
                    }

                    }
                }
                div {
                    id: "{stage_id}",
                    class: if zoom().is_none() { "mk-image-stage mk-image-fit" } else { "mk-image-stage" },
                    tabindex: "0",
                    onwheel: moonkale_ext_api::crash::event(move |e: dioxus::prelude::WheelEvent| {
                        if moonkale_ext_api::keys::primary(&e.modifiers()) {
                            e.prevent_default();
                            let dy = e.delta().strip_units().y;
                            step(if dy < 0.0 { 1 } else { -1 });
                        }
                    }),
                    onkeydown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::KeyboardEvent| {
                        match e.key() {
                            Key::Character(c) if c == "+" || c == "=" => step(1),
                            Key::Character(c) if c == "-" => step(-1),
                            Key::Character(c) if c == "0" => zoom.set(None),
                            Key::Character(c) if c == "1" => zoom.set(Some(1.0)),
                            _ => {}
                        }
                    }),
                    onmousedown: moonkale_ext_api::crash::event(move |e: dioxus::prelude::MouseEvent| {
                        if zoom().is_some() {
                            let c = e.client_coordinates();
                            drag.set(Some((c.x, c.y)));
                        }
                    }),
                    onmouseup: moonkale_ext_api::crash::event(move |_| drag.set(None)),
                    onmouseleave: moonkale_ext_api::crash::event(move |_| drag.set(None)),
                    onmousemove: moonkale_ext_api::crash::event({
                        let stage_id = stage_id_for_move.clone();
                        move |e: dioxus::prelude::MouseEvent| {
                            if let Some((x0, y0)) = drag() {
                                let c = e.client_coordinates();
                                let (dx, dy) = (c.x - x0, c.y - y0);
                                drag.set(Some((c.x, c.y)));
                                let js = format!(
                                    "(() => {{ const s = document.getElementById({id:?}); if (s) {{ s.scrollLeft -= {dx}; s.scrollTop -= {dy}; }} }})();",
                                    id = stage_id
                                );
                                let _ = dioxus::document::eval(&js);
                            }
                        }
                    }),
                    match state() {
                        Loaded::Loading => rsx! { p { class: "mk-image-note", "Loading…" } },
                        Loaded::Failed(e) => rsx! { p { class: "mk-image-note mk-image-error", "{e}" } },
                        Loaded::Ready { url, .. } => rsx! {
                            img {
                                class: "mk-image-img",
                                src: "{url}",
                                alt: "{title}",
                                draggable: "false",
                                style: "{img_style}",
                                // `load` does not bubble, so it never reaches the
                                // delegated listener (P-094); ask after mount and
                                // wait for the decode instead.
                                onmounted: moonkale_ext_api::crash::event(move |_| {
                                    // Natural size for the 100 % / zoom modes.
                                    let stage = stage_id_for_load.clone();
                                    moonkale_ext_api::crash::spawn(async move {
                                        // An eval is a function body: `return` the value.
                                        let js = format!(
                                            "const i = document.querySelector('#' + CSS.escape({id:?}) + ' img'); if (!i) return [0, 0]; try {{ await i.decode(); }} catch (_) {{}} return [i.naturalWidth, i.naturalHeight];",
                                            id = stage
                                        );
                                        if let Ok(v) = dioxus::document::eval(&js).await {
                                            if let Some(a) = v.as_array() {
                                                let w = a.first().and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                                let h = a.get(1).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                                if w > 0 && h > 0 {
                                                    natural.set(Some((w, h)));
                                                }
                                            }
                                        }
                                    });
                                }),
                            }
                        },
                    }
                }
            }
        }
}

fn human(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// A thumbnail of the image already decoded by this panel. No source read,
/// URL allocation, second decoder or screenshot of the panel is involved.
#[component]
pub(crate) fn ImagePreview(node: moonkale_core::NodeId) -> Element {
    let id = format!("mk-overview-image-{node}");
    rsx! { canvas { id, width: 256, height: 120, onmounted: move |_| {
        // NodeId's Display is a canonical UUID, never document text.

        moonkale_ext_api::crash::spawn(async move {
            let mut eval = document::eval(&format!(r#"
                const stage='mk-image-stage-{node}',target='mk-overview-image-{node}';
                const canvas=document.getElementById(target);
                const deadline=performance.now()+750;
                while(canvas?.isConnected && performance.now()<deadline) {{
                    const image=document.getElementById(stage)?.querySelector('img.mk-image-img');
                    if(image?.complete && image.naturalWidth>0 && image.naturalHeight>0) {{
                        const scale=Math.min(256/image.naturalWidth,120/image.naturalHeight);
                        const width=image.naturalWidth*scale,height=image.naturalHeight*scale;
                        canvas.getContext('2d').drawImage(image,(256-width)/2,(120-height)/2,width,height);
                        canvas.dataset.imagePreview='true';
                        break;
                    }}
                    await new Promise(resolve=>setTimeout(resolve,25));
                }}
                if(canvas?.isConnected && !canvas.dataset.imagePreview) {{
                    const ctx=canvas.getContext('2d');ctx.fillStyle=getComputedStyle(canvas).color;
                    ctx.font='24px sans-serif';ctx.fillText('◇',116,65);
                }}
                dioxus.send(true);
            "#));
            let _ = eval.recv::<bool>().await;
        });
    } } }
}
