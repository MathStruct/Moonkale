//! *Connect to Server…* (Milestone 12): a Moonkale server's URL and its
//! token; the app becomes that server's client (sources, terminal, LSP,
//! git and agent sessions there; the editor and the LLM keys here).

use dioxus::prelude::*;
use moonkale_ext_api::Workspace;

#[component]
pub fn ServerDialog(open: Signal<bool>) -> Element {
    let mut open = open;
    let ws = use_context::<Workspace>();
    let mut url = use_signal(|| {
        ws.server_link
            .peek()
            .as_ref()
            .map(|(u, _)| u.clone())
            .unwrap_or_default()
    });
    let mut token = use_signal(String::new);
    let mut connect = move || {
        let (u, t) = (url.peek().clone(), token.peek().clone());
        open.set(false);
        spawn(async move {
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
                div { class: "mk-remote-title", "Connect to Server" }
                p { class: "mk-remote-hint",
                    "Use a running Moonkale server (" code { "moonkale-server" } " or " code { "dx serve" } " with " code { "MOONKALE_TOKEN" } ") from this app: its folder, index, git, language servers, terminals and agent sessions run there; the editor and your API keys stay here. Over the network the server should use HTTPS or a tunnel — see Remote and Server Modes."
                }
                label { class: "mk-remote-field",
                    span { "Server URL" }
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
                    span { "Access token (MOONKALE_TOKEN)" }
                    input {
                        class: "mk-palette-input mk-server-token",
                        r#type: "password",
                        placeholder: "empty for a dev server without a token",
                        value: "{token}",
                        oninput: move |e| token.set(e.value()),
                    }
                }
                div { class: "mk-remote-actions",
                    button { r#type: "button", class: "mk-button", onclick: move |_| open.set(false), "Cancel" }
                    button { r#type: "submit", class: "mk-button mk-button-primary", "Connect" }
                }
            }
        }
    }
}
