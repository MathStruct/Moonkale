//! Reusable three-way text conflict model and editor. The host owns persistence
//! and operation controls; proposal conflicts can supply the same text/callbacks.
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Workspace};

pub use crate::conflict_model::{ConflictDocument, Hunk};

#[component]
pub fn ThreeWayEditor(
    ws: Workspace,
    base: String,
    ours: String,
    theirs: String,
    hunks: String,
    original: String,
    busy: bool,
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
        return rsx! { p { role: "alert", {t!(ws, L, "git-conflict-invalid")} } };
    };
    // Event callbacks share the model; cloning it per hunk would multiply
    // a multi-megabyte conflict by the number of buttons.
    let doc = std::rc::Rc::new(doc);
    let unresolved = doc.unresolved(&result.read());
    rsx! {
        div { class: "mk-three-way",
            div { class: "mk-three-way-sides",
                label { {t!(ws, L, "git-ours")} pre { "{ours}" } }
                label { {t!(ws, L, "git-base")} pre { "{base}" } }
                label { {t!(ws, L, "git-theirs")} pre { "{theirs}" } }
            }
            for (i, h) in doc.hunks.iter().enumerate() {
                div { key: "{i}", class: "mk-conflict-hunk", "data-hunk": "{i}",
                    span { {t!(ws, L, "git-hunk", n = i+1)} }
                    for (side, label) in [(0, t!(ws, L, "git-ours")), (1, t!(ws, L, "git-base")), (2, t!(ws, L, "git-theirs"))] {
                        button { class: "mk-btn", disabled: busy || !result.read().contains(&h.marker),
                            onclick: moonkale_ext_api::crash::event({ let doc = doc.clone(); move |_| { let next = doc.pick(&result.peek(), i, side); result.set(next); } }), "{label}"
                        }
                    }
                }
            }
            label { {t!(ws, L, "git-result")}
                textarea { class: "mk-conflict-result mk-input", value: "{result}", disabled: busy,
                    oninput: moonkale_ext_api::crash::event(move |e: FormEvent| result.set(e.value())),
                    onkeydown: moonkale_ext_api::crash::event(move |e: KeyboardEvent| e.stop_propagation())
                }
            }
            button { class: "mk-btn", disabled: busy || unresolved, onclick: moonkale_ext_api::crash::event(move |_| onresolve.call(result.peek().clone())), {t!(ws, L, "git-resolve")} }
        }
    }
}
