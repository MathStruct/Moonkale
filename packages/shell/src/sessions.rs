//! A quiet title-bar/palette switcher; destructive intent requires a second click.
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Command, Workspace};

#[component]
pub(crate) fn Sessions(mut ws: Workspace) -> Element {
    let mut open = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut deleting = use_signal(|| false);
    let mut handled = use_signal(|| 0);
    let mut bundle = use_signal(String::new);
    let mut transferring = use_signal(|| false);
    use_effect(move || {
        let (seq, command) = *ws.shell.commands.read();
        if seq != *handled.peek() {
            handled.set(seq);
            if command == Some(Command::Sessions) {
                error.set(String::new());
                deleting.set(false);
                transferring.set(false);
                open.set(true);
                name.set(ws.session_name());
            }
        }
    });
    // Metadata write-through, outside the document input path. Explicit
    // switching awaits its own save and periodic ticks cover panel state.
    use_effect(move || {
        let mut timer = document::eval("const t=setInterval(()=>dioxus.send(true),1000); dioxus.send(true); await new Promise(()=>{});");
        spawn(async move {
            while timer.recv::<bool>().await.is_ok() {
                if let Err(e) = ws.persist_session().await {
                    ws.set_status(e);
                }
            }
        });
    });
    if !open() {
        return rsx! {};
    }
    let active = ws.saved_sessions.active.read().clone();
    let list = ws.saved_sessions.list.read().clone();
    rsx! {
        div { class: "mk-palette-backdrop", onclick: move |_| open.set(false),
            div { class: "mk-palette mk-remote-dialog mk-sessions", role: "dialog", "aria-modal": "true",
                aria_label: t!(ws, L, "sessions-title"),
                onclick: |e| e.stop_propagation(),
                onkeydown: move |e| { if e.key() == Key::Escape { open.set(false); } },
                h3 { {t!(ws, L, "sessions-title")} }
                div { class: "mk-palette-list",
                    for session in list.iter().filter(|s| !s.archived) {
                        button { class: "mk-palette-item", key: "{session.id}",
                            title: session.sources.iter().map(|s| s.display_name.clone()).collect::<Vec<_>>().join(" · "),
                            "data-session": "{session.id}", disabled: *ws.saved_sessions.switching.read(),
                            onclick: {
                                let id = session.id.clone();
                                move |_| { let id = id.clone(); spawn(async move {
                                    match ws.switch_session(id).await { Ok(()) => open.set(false), Err(e) => error.set(e) }
                                }); }
                            },
                            if session.id == active { "● " }
                            "{session.name}"
                            small { {t!(ws, L, "sessions-detail", sources = session.sources.len(), minutes = moonkale_ext_api::workspace::now_ms().saturating_sub(session.touched) / 60000)} }
                        }
                    }
                }
                form { onsubmit: move |e| { e.prevent_default(); let value = name.peek().clone(); spawn(async move {
                        match ws.new_session(value).await { Ok(_) => open.set(false), Err(e) => error.set(e) }
                    }); },
                    input { class: "mk-palette-input", id: "mk-session-name", autofocus: true,
                        aria_label: t!(ws, L, "sessions-name"), value: "{name}", maxlength: "128",
                        oninput: move |e| name.set(e.value()), }
                    button { class: "mk-button", r#type: "submit", {t!(ws, L, "sessions-new")} }
                    button { class: "mk-button", r#type: "button", onclick: move |_| {
                        let id = ws.saved_sessions.active.peek().clone(); let value = name.peek().clone();
                        spawn(async move { if let Err(e) = ws.rename_session(&id, value).await { error.set(e); } });
                    }, {t!(ws, L, "sessions-rename")} }
                    button { class: "mk-button", r#type: "button", disabled: active == "default", onclick: move |_| deleting.set(true), {t!(ws, L, "sessions-delete")} }
                }
                if deleting() {
                    p { {t!(ws, L, "sessions-delete-confirm")} }
                    button { class: "mk-button", onclick: move |_| { let id = ws.saved_sessions.active.peek().clone(); spawn(async move {
                        match ws.delete_session(&id).await { Ok(()) => { deleting.set(false); open.set(false); }, Err(e) => error.set(e) }
                    }); }, {t!(ws, L, "sessions-delete-confirm-button")} }
                    button { class: "mk-button", onclick: move |_| deleting.set(false), {t!(ws, L, "cancel")} }
                }
                button { class: "mk-button", onclick: move |_| {
                    match ws.export_session() { Ok(text) => {
                        let text = serde_json::to_string(&text).unwrap();
                        let _ = document::eval(&format!("const blob=new Blob([{text}],{{type:'application/json'}}); const url=URL.createObjectURL(blob); const a=document.createElement('a'); a.href=url; a.download='moonkale-session.json'; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000);"));
                    }, Err(e) => error.set(e) }
                }, {t!(ws, L, "sessions-export")} }
                button { class: "mk-button", onclick: move |_| transferring.set(!transferring()), {t!(ws, L, "sessions-import")} }
                if transferring() {
                    input { r#type: "file", accept: ".json,application/json", aria_label: t!(ws, L, "sessions-import"),
                        onchange: move |event: FormEvent| { let files = event.files(); spawn(async move {
                            if let Some(file) = files.first() {
                                if file.size() > 4 * 1024 * 1024 { error.set(t!(ws, L, "sessions-invalid")); return; }
                                match file.read_bytes().await {
                                    Ok(bytes) if bytes.len() <= 4 * 1024 * 1024 => match String::from_utf8(bytes.to_vec()) {
                                        Ok(text) => bundle.set(text), Err(_) => error.set(t!(ws, L, "sessions-invalid"))
                                    }, _ => error.set(t!(ws, L, "sessions-invalid"))
                                }
                            }
                        }); }, }
                    textarea { class: "mk-palette-input", aria_label: t!(ws, L, "sessions-bundle"), value: "{bundle}",
                        oninput: move |e| bundle.set(e.value()), }
                    button { class: "mk-button", onclick: move |_| { let text = bundle.peek().clone(); spawn(async move {
                        match ws.import_session(&text).await { Ok(_) => { bundle.set(String::new()); transferring.set(false); }, Err(_) => error.set(t!(ws, L, "sessions-invalid")) }
                    }); }, {t!(ws, L, "sessions-import-confirm")} }
                }
                if !error().is_empty() { p { role: "alert", "{error}" } }
                button { class: "mk-button", onclick: move |_| open.set(false), {t!(ws, L, "recovery-close")} }
            }
        }
    }
}
