//! *Connect to Server…* (Milestone 12): a Moonkale server's URL and its
//! token; the app becomes that server's client (sources, terminal, LSP,
//! git and agent sessions there; the editor and the LLM keys here).

use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Workspace};

#[component]
pub fn ServerDialog(open: Signal<bool>) -> Element {
    let mut open = open;
    let ws = use_context::<Workspace>();
    let mut url = use_signal(|| {
        ws.remote
            .server
            .peek()
            .as_ref()
            .map(|(u, _)| u.clone())
            .unwrap_or_default()
    });
    let mut token = use_signal(String::new);
    let mut connect = move || {
        let (u, t) = (url.peek().clone(), token.peek().clone());
        open.set(false);
        // The dialog unmounts with `open`; a task of its own scope would be
        // cancelled with it (P-108, P-116).
        dioxus::core::spawn_forever(async move {
            ws.connect_server(u, Some(t)).await;
        });
    };
    rsx! {
        div {
            class: "mk-palette-backdrop",
            onclick: move |_| open.set(false),
            form {
                class: "mk-palette mk-remote-dialog mk-server-dialog",
                onclick: move |e| e.stop_propagation(),
                onsubmit: move |e| { e.prevent_default(); connect(); },
                onkeydown: move |e| { if e.key() == Key::Escape { open.set(false); } },
                div { class: "mk-remote-title", {t!(ws, L, "server-title")} }
                p { class: "mk-remote-hint", {t!(ws, L, "server-hint")} }
                label { class: "mk-remote-field",
                    span { {t!(ws, L, "server-url")} }
                    input {
                        class: "mk-palette-input mk-server-url",
                        r#type: "url",
                        placeholder: "http://127.0.0.1:8080 · https://box.example:8443",
                        autofocus: true,
                        value: "{url}",
                        oninput: move |e| url.set(e.value()),
                    }
                }
                label { class: "mk-remote-field",
                    span { {t!(ws, L, "server-token")} }
                    input {
                        class: "mk-palette-input mk-server-token",
                        r#type: "password",
                        placeholder: t!(ws, L, "server-token-placeholder"),
                        value: "{token}",
                        oninput: move |e| token.set(e.value()),
                    }
                }
                div { class: "mk-remote-actions",
                    button { r#type: "button", class: "mk-button", onclick: move |_| open.set(false), {t!(ws, L, "cancel")} }
                    button { r#type: "submit", class: "mk-button mk-button-primary", {t!(ws, L, "connect")} }
                }
            }
        }
    }
}
