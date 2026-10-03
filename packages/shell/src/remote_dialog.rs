//! "Open Remote Folder…" (Milestone 11): host and path, then the workspace
//! starts the SSH session. Hosts come from `~/.ssh/config`; the `ssh`
//! process appears as a terminal tab and asks there for whatever it needs.

use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Workspace};

#[component]
pub fn RemoteDialog(open: Signal<bool>) -> Element {
    let mut open = open;
    let ws = use_context::<Workspace>();
    let hosts = use_memo(move || ws.remote_hosts());
    let mut host = use_signal(|| {
        ws.remote
            .ssh
            .peek()
            .as_ref()
            .map(|r| r.host.clone())
            .unwrap_or_default()
    });
    let mut path = use_signal(|| {
        ws.remote
            .ssh
            .peek()
            .as_ref()
            .map(|r| r.path.clone())
            .unwrap_or_default()
    });
    // Saved connections (Milestone 15): pick one to fill the fields, save
    // the fields under a name, forget one.
    let saved = use_memo(move || ws.settings.resolved.read().remote_saved.clone());
    let mut picked = use_signal(String::new);
    let mut save_name = use_signal(String::new);
    let mut connect = move || {
        let (h, p) = (host.peek().clone(), path.peek().clone());
        open.set(false);
        ws.open_remote(h, p);
    };
    let mut pick = move |name: String| {
        picked.set(name.clone());
        save_name.set(name.clone());
        if let Some(c) = saved.peek().iter().find(|c| c.name == name) {
            host.set(c.host.clone());
            path.set(c.path.clone());
        }
    };
    let save = move |_| {
        let (n, h, p) = (
            save_name.peek().trim().to_string(),
            host.peek().clone(),
            path.peek().clone(),
        );
        if n.is_empty() {
            let mut ws = ws;
            ws.set_status(t!(ws, L, "remote-name-needed"));
            return;
        }
        picked.set(n.clone());
        // Outlives the dialog if it closes meanwhile (P-108).
        dioxus::core::spawn_forever(async move {
            ws.save_remote(n, h, p).await;
        });
    };
    let forget = move |_| {
        let n = picked.peek().clone();
        if n.is_empty() {
            return;
        }
        picked.set(String::new());
        save_name.set(String::new());
        dioxus::core::spawn_forever(async move {
            ws.forget_remote(n).await;
        });
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
                div { class: "mk-remote-title", {t!(ws, L, "remote-title")} }
                p { class: "mk-remote-hint", {t!(ws, L, "remote-hint-ssh")} }
                p { class: "mk-remote-hint", {t!(ws, L, "remote-hint-host")} }
                p { class: "mk-remote-hint", {t!(ws, L, "remote-hint-server")} }
                if !saved.read().is_empty() {
                    label { class: "mk-remote-field",
                        span { {t!(ws, L, "remote-saved")} }
                        select {
                            class: "mk-palette-input mk-remote-saved",
                            value: "{picked}",
                            onchange: move |e| pick(e.value()),
                            option { value: "", selected: picked().is_empty(), {t!(ws, L, "remote-pick")} }
                            for c in saved.read().iter() {
                                option { key: "{c.name}", value: "{c.name}", selected: picked() == c.name, "{c.name} — {c.host}:{c.path}" }
                            }
                        }
                    }
                }
                label { class: "mk-remote-field",
                    span { {t!(ws, L, "remote-host")} }
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
                    span { {t!(ws, L, "remote-folder")} }
                    input {
                        class: "mk-palette-input mk-remote-path",
                        r#type: "text",
                        placeholder: "/home/me/project",
                        value: "{path}",
                        oninput: move |e| path.set(e.value()),
                    }
                }
                div { class: "mk-remote-save",
                    input {
                        class: "mk-palette-input mk-remote-save-name",
                        r#type: "text",
                        placeholder: t!(ws, L, "remote-save-name"),
                        value: "{save_name}",
                        oninput: move |e| save_name.set(e.value()),
                    }
                    button { r#type: "button", class: "mk-button mk-remote-save-btn", onclick: save, title: t!(ws, L, "remote-save-title"), {t!(ws, L, "menu-save")} }
                    if !picked().is_empty() {
                        button { r#type: "button", class: "mk-button mk-remote-forget-btn", onclick: forget, {t!(ws, L, "remote-forget")} }
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
