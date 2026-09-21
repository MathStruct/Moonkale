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
                    "The folder is opened through your system "
                    code { "ssh" }
                    ": keys, agent, passwords and host checks work exactly as in a terminal, and every prompt shows up in the Terminal panel. Moonkale's server is copied to the host once per version (into "
                    code { "~/.local/share/moonkale" }
                    ") and runs only for this session."
                }
                label { class: "mk-remote-field",
                    span { "Host" }
                    input {
                        class: "mk-palette-input mk-remote-host",
                        r#type: "text",
                        placeholder: "build-box or user@10.0.0.2",
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
