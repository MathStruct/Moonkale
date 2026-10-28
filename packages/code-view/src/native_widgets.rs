//! View-only block content. Controls own focus and never bubble into editor input.
use dioxus::prelude::*;

#[component]
pub(crate) fn PreviewBlock(
    identity: String,
    title: String,
    show_label: String,
    hide_label: String,
    source_label: String,
    onsource: Callback<()>,
    children: Element,
) -> Element {
    let mut expanded = use_signal(|| false);
    let body_id = format!("{identity}-body");
    rsx! {
        div {
            class: "mk-preview-widget",
            "data-widget-id": identity,
            onmousedown: moonkale_ext_api::crash::event(move |event: dioxus::prelude::MouseEvent| event.stop_propagation()),
            onmousemove: moonkale_ext_api::crash::event(move |event: dioxus::prelude::MouseEvent| event.stop_propagation()),
            onclick: moonkale_ext_api::crash::event(move |event: dioxus::prelude::MouseEvent| event.stop_propagation()),
            onkeydown: moonkale_ext_api::crash::event(move |event: dioxus::prelude::KeyboardEvent| {
                event.stop_propagation();
                if event.key() == Key::Escape && !event.is_composing() {
                    event.prevent_default();
                    onsource.call(());
                }
            }),
            oncopy: moonkale_ext_api::crash::event(move |event: dioxus::prelude::ClipboardEvent| event.stop_propagation()),
            oncut: moonkale_ext_api::crash::event(move |event: dioxus::prelude::ClipboardEvent| event.stop_propagation()),
            onpaste: moonkale_ext_api::crash::event(move |event: dioxus::prelude::ClipboardEvent| event.stop_propagation()),
            div { class: "mk-preview-widget-header",
                span { "{title}" }
                button {
                    r#type: "button",
                    class: "mk-btn mk-preview-widget-toggle",
                    aria_expanded: expanded().to_string(),
                    aria_controls: body_id.clone(),
                    onclick: moonkale_ext_api::crash::event(move |_| expanded.toggle()),
                    if expanded() { "{hide_label}" } else { "{show_label}" }
                }
                button {
                    r#type: "button",
                    class: "mk-btn mk-preview-widget-source",
                    onclick: moonkale_ext_api::crash::event(move |_| onsource.call(())),
                    "{source_label}"
                }
            }
            // Keep content mounted while collapsed so view-local input survives toggles.
            div { id: body_id, class: "mk-preview-widget-body", hidden: !expanded(), {children} }
        }
    }
}

/// Acceptance content supplied by a provider, rather than built into PreviewBlock.
#[component]
pub(crate) fn FixtureWidgetBody() -> Element {
    let mut note = use_signal(String::new);
    rsx! {
        p { "Preview content: <safe> 😀中" }
        label { "Local note "
            input {
                class: "mk-preview-widget-note",
                // Native input owns its value; delayed Rust renders must not overwrite newer keystrokes.
                initial_value: "",
                oninput: moonkale_ext_api::crash::event(move |event: dioxus::prelude::FormEvent| note.set(event.value())),
            }
        }
        p { class: "mk-preview-widget-note-value", "{note}" }
    }
}
