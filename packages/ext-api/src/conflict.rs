//! Reusable three-way text conflict model and editor. The host owns persistence
//! and operation controls; proposal conflicts can supply the same text/callbacks.
use crate::L;
use crate::{t, Workspace};
use dioxus::prelude::*;

pub use crate::conflict_model::{ConflictDocument, Hunk};

/// Review three text versions; resolving returns text and never writes it.
#[component]
pub fn ThreeWayEditor(
    ws: Workspace,
    base: String,
    ours: String,
    theirs: String,
    hunks: String,
    original: String,
    busy: bool,
    #[props(default)] whole_file: bool,
    onresolve: EventHandler<String>,
) -> Element {
    let parsed = ConflictDocument::parse(&hunks);
    let mut result = use_signal(|| {
        parsed
            .as_ref()
            .map(|d| d.initialize(&original))
            .unwrap_or(original)
    });
    let Ok(doc) = parsed else {
        return rsx! { p { role: "alert", {t!(ws, L, "conflict-conflict-invalid")} } };
    };
    // Event callbacks share the model; cloning it per hunk would multiply
    // a multi-megabyte conflict by the number of buttons.
    let doc = std::rc::Rc::new(doc);
    let unresolved = doc.unresolved(&result.read());
    rsx! {
        crate::Stylesheet { href: asset!("/assets/conflict.css") }
        div { class: "mk-three-way",
            div { class: "mk-three-way-sides",
                label { {t!(ws, L, "conflict-ours")} pre { "{ours}" } }
                label { {t!(ws, L, "conflict-base")} pre { "{base}" } }
                label { {t!(ws, L, "conflict-theirs")} pre { "{theirs}" } }
            }
            if whole_file {
                for (text, label) in [(ours, t!(ws,L,"conflict-ours")), (base, t!(ws,L,"conflict-base")), (theirs, t!(ws,L,"conflict-theirs"))] {
                    button { class: "mk-btn", disabled: busy,
                        onclick: crate::crash::event(move |_| result.set(text.clone())), "{label}"
                    }
                }
            }
            for (i, h) in doc.hunks.iter().enumerate() {
                div { key: "{i}", class: "mk-conflict-hunk", "data-hunk": "{i}",
                    span { {t!(ws, L, "conflict-hunk", n = i+1)} }
                    for (side, label) in [(0, t!(ws, L, "conflict-ours")), (1, t!(ws, L, "conflict-base")), (2, t!(ws, L, "conflict-theirs"))] {
                        button { class: "mk-btn", disabled: busy || !result.read().contains(&h.marker),
                            onclick: crate::crash::event({ let doc = doc.clone(); move |_| { let next = doc.pick(&result.peek(), i, side); result.set(next); } }), "{label}"
                        }
                    }
                }
            }
            label { {t!(ws, L, "conflict-result")}
                textarea { class: "mk-conflict-result mk-input", value: "{result}", disabled: busy,
                    oninput: crate::crash::event(move |e: FormEvent| result.set(e.value())),
                    onkeydown: crate::crash::event(move |e: KeyboardEvent| e.stop_propagation())
                }
            }
            button { class: "mk-btn", disabled: busy || unresolved, onclick: crate::crash::event(move |_| onresolve.call(result.peek().clone())), {t!(ws, L, "conflict-resolve")} }
        }
    }
}
