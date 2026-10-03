//! Which terminal? (Milestone 12) Both terminal extensions are enabled and
//! `terminal.implementation` is `ask`: pick one for this terminal, and
//! optionally remember it.

use dioxus::prelude::*;
use moonkale_ext_api::Workspace;

#[component]
pub fn TerminalChooser(open: Signal<bool>) -> Element {
    let mut open = open;
    let ws = use_context::<Workspace>();
    let mut remember = use_signal(|| false);
    let mut pick = move |which: &'static str| {
        open.set(false);
        if remember() {
            let w = which.to_string();
            // Outlives the dialog (P-108).
            dioxus::core::spawn_forever(async move {
                ws.update_user_settings(|f| f.terminal.implementation = Some(w))
                    .await;
            });
        }
        crate::frame::open_terminal_in(ws, which);
    };
    rsx! {
        div {
            class: "mk-palette-backdrop",
            onclick: move |_| open.set(false),
            div {
                class: "mk-palette mk-remote-dialog mk-terminal-chooser",
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e| { if e.key() == Key::Escape { open.set(false); } },
                div { class: "mk-remote-title", "Open the terminal with…" }
                p { class: "mk-remote-hint", "Two terminal panels are enabled. Settings → Terminal → Implementation makes this permanent." }
                div { class: "mk-terminal-choices",
                    button { r#type: "button", class: "mk-button mk-button-primary", autofocus: true, onclick: move |_| pick("xterm"), "xterm.js" span { class: "mk-muted", " — the JavaScript terminal (default)" } }
                    button { r#type: "button", class: "mk-button", onclick: move |_| pick("native"), "Rust" span { class: "mk-muted", " — the Dioxus-rendered terminal (no JavaScript)" } }
                }
                label { class: "mk-settings-check",
                    input { r#type: "checkbox", checked: remember(), onchange: move |e| remember.set(e.checked()) }
                    "Remember my choice"
                }
                div { class: "mk-remote-actions",
                    button { r#type: "button", class: "mk-button", onclick: move |_| open.set(false), "Cancel" }
                }
            }
        }
    }
}
