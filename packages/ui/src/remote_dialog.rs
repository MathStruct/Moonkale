//! "Open Remote Folder…" (Milestone 11): host and path, then the workspace
//! starts the SSH session. Hosts come from `~/.ssh/config`; the `ssh`
//! process appears as a terminal tab and asks there for whatever it needs.

use dioxus::prelude::*;
use moonkale_ext_api::Workspace;

#[component]
pub fn RemoteDialog(open: Signal<bool>) -> Element {
    let mut open = open;
    let ws = use_context::<Workspace>();
    let hosts = use_memo(move || ws.remote_hosts());
    let mut host = use_signal(|| {
        ws.remote
            .peek()
            .as_ref()
            .map(|r| r.host.clone())
            .unwrap_or_default()
    });
    let mut path = use_signal(|| {
        ws.remote
            .peek()
            .as_ref()
            .map(|r| r.path.clone())
            .unwrap_or_default()
    });
    let mut connect = move || {
        let (h, p) = (host.peek().clone(), path.peek().clone());
        open.set(false);
        ws.open_remote(h, p);
    };
    rsx! {
        div {
            class: "mk-palette-backdrop",
            onclick: move |_| open.set(false),
            form {
                class: "mk-palette mk-remote-dialog",
                onclick: move |e| e.stop_propagation(),
                onsubmit: move |e| { e.prevent_default(); connect(); },
                onkeydown: move |e| { if e.key() == Key::Escape { open.set(false); } },
                div { class: "mk-remote-title", "Open Remote Folder" }
                p { class: "mk-remote-hint",
                    "Opens a folder on another machine through your system "
                    code { "ssh" }
                    ". Authentication is ssh's own — keys, agent, passwords and host-key checks work exactly as in a terminal; whatever ssh asks appears in the Terminal panel, and Moonkale never sees a secret."
                }
                p { class: "mk-remote-hint",
                    b { "Host" }
                    ": what you would type after "
                    code { "ssh" }
                    " — a name or "
                    code { "user@host" }
                    ", an alias from "
                    code { "~/.ssh/config" }
                    " (offered below), with any ssh options in front ("
                    code { "-p 2222" }
                    ", "
                    code { "-i ~/.ssh/key" }
                    ", "
                    code { "-J jumphost" }
                    ", "
                    code { "-o …" }
                    "); "
                    code { "VAR=value" }
                    " words at the start are set in ssh's environment."
                }
                p { class: "mk-remote-hint",
                    "The first time per host and version, Moonkale's own server (about 150 MB) is copied to the host into "
                    code { "~/.local/share/moonkale/server/" }
                    " and started there for this session only: the folder, its index, git, language servers and terminals then run on that machine; the editor and your API keys stay here. Closing the folder ends the session and the server."
                }
                label { class: "mk-remote-field",
                    span { "Host (as typed after ssh)" }
                    input {
                        class: "mk-palette-input mk-remote-host",
                        r#type: "text",
                        placeholder: "user@host  ·  alias  ·  -p 2222 -i ~/.ssh/key user@host",
                        list: "mk-remote-hosts",
                        autofocus: true,
                        value: "{host}",
                        oninput: move |e| host.set(e.value()),
                    }
                    datalist { id: "mk-remote-hosts",
                        for h in hosts.read().iter() {
                            option { value: "{h}" }
                        }
                    }
                }
                label { class: "mk-remote-field",
                    span { "Folder on that machine" }
                    input {
                        class: "mk-palette-input mk-remote-path",
                        r#type: "text",
                        placeholder: "/home/me/project",
                        value: "{path}",
                        oninput: move |e| path.set(e.value()),
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
