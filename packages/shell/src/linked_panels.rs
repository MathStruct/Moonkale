//! Link-group tab presentation and pointer/keyboard routes through host identities.
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{t, Command, Workspace};
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    kind: String,
    panel: String,
    #[serde(default)]
    target: String,
}

#[component]
pub(crate) fn LinkedPanels(mut ws: Workspace, panels: Vec<(String, String)>) -> Element {
    let mut menu = use_signal(|| None::<String>);
    let mut choosing = use_signal(|| false);
    let mut pinned = use_signal(|| false);
    let mut handled = use_signal(|| 0);
    let known = panels.clone();
    use_effect(move || {
        let (seq, command) = *ws.shell.commands.read();
        if seq == *handled.peek() {
            return;
        }
        handled.set(seq);
        match command {
            Some(Command::OpenWithContext) => {
                if let Some(node) = (*ws.docs.active.peek()).and_then(|id| ws.open_node_by_id(id)) {
                    spawn(async move {
                        if let Err(e) = ws.open_with_context(node).await {
                            ws.set_status(e.to_string());
                        }
                    });
                }
            }
            Some(Command::LinkPanel) | Some(Command::UnlinkPanel) => {
                let panel = ws
                    .links
                    .focused
                    .peek()
                    .clone()
                    .or_else(|| (*ws.docs.active.peek()).map(|n| ws.content_panel(n)));
                if let Some(panel) = panel {
                    if command == Some(Command::UnlinkPanel) {
                        ws.unlink_panel(&panel);
                    } else {
                        choosing.set(true);
                        menu.set(Some(panel));
                    }
                }
            }
            _ => {}
        }
    });
    // Every payload is checked against the current contribution registry.
    use_effect(use_reactive((&known,), move |(known,)| {
        let payload = serde_json::to_string(&known).unwrap_or_default();
        spawn(async move {
            let mut eval = document::eval(&SCRIPT.replace("PANEL_DATA", &payload));
            while let Ok(input) = eval.recv::<Input>().await {
                if !known.iter().any(|(id, _)| id == &input.panel) {
                    continue;
                }
                match input.kind.as_str() {
                    "focus" => {
                        if ws.links.focused.peek().as_deref() != Some(&input.panel) {
                            ws.links.focused.set(Some(input.panel));
                        }
                    }
                    "menu" => {
                        choosing.set(false);
                        menu.set(Some(input.panel));
                    }
                    "link" if known.iter().any(|(id, _)| id == &input.target) => {
                        ws.link_panel(&input.panel, &input.target, false)
                    }
                    _ => {}
                }
            }
        });
    }));
    let mark_data: Vec<_> = panels
        .iter()
        .map(|(id, title)| {
            let group = ws.panel_group(id);
            let label = group.as_ref().map(|g| {
                let names = g
                    .members
                    .iter()
                    .filter(|(member, _, _)| member != id)
                    .filter_map(|(member, _, _)| {
                        panels
                            .iter()
                            .find(|(p, _)| p == member)
                            .map(|(_, t)| t.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                t!(ws, L, "link-description", names = names, number = g.number)
            });
            (
                id.clone(),
                title.clone(),
                group.as_ref().map(|g| g.colour),
                group.as_ref().map(|g| g.id.clone()),
                label,
            )
        })
        .collect();
    use_effect(use_reactive((&mark_data,), move |(marks,)| {
        let payload = serde_json::to_string(&marks).unwrap_or_default();
        let _ = document::eval(&format!("window.moonkaleLinkMarks = {payload}; window.moonkaleMarkLinks?.(window.moonkaleLinkMarks);"));
    }));
    rsx! {
        if let Some(panel) = menu() {
            div { class: "mk-dialog-backdrop", onclick: move |_| menu.set(None),
                div { class: "mk-dialog mk-link-menu", role: "dialog", "aria-modal": "true", "aria-label": t!(ws, L, "link-to"),
                    onclick: move |e| e.stop_propagation(),
                    onkeydown: move |e| { if e.key() == Key::Escape { e.stop_propagation(); menu.set(None); } },
                    if choosing() {
                        label { input { r#type: "checkbox", checked: pinned(), onchange: move |e| pinned.set(e.checked()) } {t!(ws, L, "link-pin")} }
                        for (id, title) in panels.iter().filter(|(id, _)| id != &panel).map(|(id,title)| (id.clone(),ws.panel_group(id).map(|g| format!("{} {title}",g.marker())).unwrap_or_else(|| title.clone()))) {
                            button { class: "mk-btn", onclick: {
                                let panel = panel.clone();
                                move |_| { ws.link_panel(&panel, &id, pinned()); menu.set(None); }
                            }, "{title}" }
                        }
                    } else {
                        button { class: "mk-btn", onclick: move |_| choosing.set(true), {t!(ws, L, "link-to")} }
                        button { class: "mk-btn", onclick: {
                            let panel = panel.clone();
                            move |_| { ws.unlink_panel(&panel); menu.set(None); }
                        }, {t!(ws, L, "link-unlink")} }
                        if let Some(node) = ws.link_subject(&panel).and_then(|s| ws.open_node_by_id(s.node)) {
                            button { class: "mk-btn", onclick: move |_| { let node = node.clone(); menu.set(None); spawn(async move { let _ = ws.open_with_context(node).await; }); }, {moonkale_ext_api::command_hint("panels.openWithContext", t!(ws, L, "link-open-context")).title} }
                        }
                    }
                    button { class: "mk-btn", onclick: move |_| menu.set(None), {t!(ws, L, "link-cancel")} }
                }
            }
        }
    }
}

const SCRIPT: &str = r#"
window.__mkLinksCleanup?.();
const panels = PANEL_DATA;
const safe = s => Array.from(s).map(c => /[a-zA-Z0-9_-]/.test(c) ? c : '-').join('');
const tabs = () => panels.map(([id]) => [id, document.getElementById('wb-tab-' + safe(id))]).filter(([,el]) => el?.classList.contains('wb-tab') && !el.closest('.wb-panel-content'));
const find = target => tabs().find(([,el]) => el === target.closest?.('.wb-tab'));
const send = (kind, panel, target='') => dioxus.send({kind,panel,target});
let dragging;
const listeners = [];
const on = (name, fn) => { document.addEventListener(name,fn,true); listeners.push([name,fn]); };
on('focusin', e => { const entry = find(e.target) || tabs().find(([,el]) => document.getElementById(el.getAttribute('aria-controls'))?.contains(e.target)); if(entry) send('focus',entry[0]); });
on('pointerdown', e => { const entry = find(e.target) || tabs().find(([,el]) => document.getElementById(el.getAttribute('aria-controls'))?.contains(e.target)); if(entry) send('focus',entry[0]); });
on('contextmenu', e => { const entry=find(e.target); if(entry) { e.preventDefault(); e.stopPropagation(); send('menu',entry[0]); } });
on('keydown', e => { if(e.key==='ContextMenu' || (e.shiftKey && e.key==='F10')) { const entry=find(e.target); if(entry) { e.preventDefault(); e.stopPropagation(); send('menu',entry[0]); } } });
on('dragstart', e => { dragging=find(e.target)?.[0]; });
on('dragover', e => { if(e.altKey && dragging && find(e.target)) { e.preventDefault(); e.stopPropagation(); } });
on('drop', e => { const entry=find(e.target); if(e.altKey && dragging && entry) { e.preventDefault(); e.stopPropagation(); send('link',dragging,entry[0]); dragging=null; } });
on('dragend', () => { dragging=null; });
on('pointerover', e => { const entry=find(e.target); if(!entry) return; const group=entry[1].dataset.linkGroup; for(const [,tab] of tabs()) tab.classList.toggle('mk-link-hover',!!group && tab.dataset.linkGroup===group); });
on('pointerout', e => { if(find(e.target)) for(const [,tab] of tabs()) tab.classList.remove('mk-link-hover'); });
let marking = false;
window.moonkaleMarkLinks = marks => { window.moonkaleLinkMarks = marks; if (marking) return; marking = true; requestAnimationFrame(() => {
  marking = false;
  for (const [id,title,colour,group,label] of (window.moonkaleLinkMarks || [])) {
    const el=tabs().find(([p])=>p===id)?.[1]; if(!el) continue;
    for(let n=1;n<=8;n++) el.classList.toggle('mk-link-'+n,colour===n);
    if(group) el.dataset.linkGroup=group; else delete el.dataset.linkGroup;
    el.setAttribute('aria-label',label ? title+', '+label : title);
    if(label) el.title=title+', '+label;
  }
}); };
const observer = new MutationObserver(() => window.moonkaleMarkLinks(window.moonkaleLinkMarks || []));
observer.observe(document.body, {childList:true,subtree:true});
window.moonkaleMarkLinks(window.moonkaleLinkMarks || []);
let finish;
const lifetime = new Promise(resolve => { finish = resolve; });
window.__mkLinksCleanup = () => { observer.disconnect(); for(const [name,fn] of listeners) document.removeEventListener(name,fn,true); finish(); };
await lifetime;
"#;
