//! Rich (WYSIWYG) markdown mode: Milkdown's Crepe editor behind
//! [`RichTextBackend`]. Rust owns the markdown (`Document`); the view emits
//! whole-document changes, like the code editor did in Milestone 1.

use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use moonkale_ext_api::Workspace;
use serde::{Deserialize, Serialize};

pub const BUNDLE: Asset = asset!("/assets/milkdown.js");
pub const BUNDLE_CSS: Asset = asset!("/assets/milkdown.css");
const CSS: Asset = asset!("/assets/rich.css");

/// The replaceable contract (a Rust-native rich text view would implement it).
pub trait RichTextBackend {
    fn set_text(&self, markdown: &str);
    fn focus(&self);
}

pub enum RichEvent {
    Ready,
    Changed(String),
    WikiLink(String),
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    Init { text: &'a str },
    SetText { text: &'a str },
    Focus,
    Destroy,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready,
    Change { text: String },
    WikiLink { target: String },
    Error { message: String },
}

const SCRIPT: &str = r#"
const el = document.getElementById(ELEMENT_ID);
if (!el) { return; }
while (!(window.moonkale && window.moonkale.milkdown)) {
    await new Promise((r) => setTimeout(r, 20));
}
const md = window.moonkale.milkdown;
const init = await dioxus.recv();
try {
    await md.mount(el, init.text, (text) => dioxus.send({ kind: "change", text }), (target) => dioxus.send({ kind: "wikiLink", target }));
} catch (e) {
    dioxus.send({ kind: "error", message: String(e && e.message ? e.message : e) });
    for (;;) { const m = await dioxus.recv(); if (m.kind === "destroy") return; }
}
dioxus.send({ kind: "ready" });
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setText") md.setText(el, msg.text);
    else if (msg.kind === "focus") md.focus(el);
    else if (msg.kind === "destroy") { md.destroy(el); break; }
}
"#;

pub struct MilkdownBackend {
    eval: Eval,
}

impl MilkdownBackend {
    pub fn mount(element_id: String, initial: String, on_event: Callback<RichEvent>) -> Self {
        let script = SCRIPT.replace("ELEMENT_ID", &serde_json::to_string(&element_id).unwrap());
        let eval = document::eval(&script);
        let _ = eval.send(ToJs::Init { text: &initial });
        let mut rx = eval;
        spawn(async move {
            loop {
                match rx.recv::<FromJs>().await {
                    Ok(FromJs::Ready) => on_event.call(RichEvent::Ready),
                    Ok(FromJs::Change { text }) => on_event.call(RichEvent::Changed(text)),
                    Ok(FromJs::WikiLink { target }) => on_event.call(RichEvent::WikiLink(target)),
                    Ok(FromJs::Error { message }) => {
                        tracing::warn!("milkdown: {message}");
                        break;
                    }
                    Err(dioxus::document::EvalError::Serialization(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        Self { eval }
    }
}

impl RichTextBackend for MilkdownBackend {
    fn set_text(&self, markdown: &str) {
        let _ = self.eval.send(ToJs::SetText { text: markdown });
    }
    fn focus(&self) {
        let _ = self.eval.send(ToJs::Focus);
    }
}

impl Drop for MilkdownBackend {
    fn drop(&mut self) {
        let _ = self.eval.send(ToJs::Destroy);
    }
}

/// The rich view of one markdown document.
#[component]
pub fn RichPanel(ws: Workspace, node: moonkale_core::NodeId) -> Element {
    let Some(mut doc) = ws.document(node) else {
        return rsx! { div { class: "mk-editor-missing", "Document is not open." } };
    };
    let element_id = format!("mk-rich-{node}");
    let mut backend: Signal<Option<std::rc::Rc<MilkdownBackend>>> = use_signal(|| None);
    let mut ready = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let mount = {
        let element_id = element_id.clone();
        move |_| {
            if backend.peek().is_some() {
                return;
            }
            let initial = doc.peek().text.clone();
            let on_event = Callback::new(move |ev: RichEvent| match ev {
                RichEvent::Ready => ready.set(true),
                RichEvent::Changed(text) => doc.with_mut(|d| d.text = text),
                RichEvent::WikiLink(target) => {
                    spawn(open_wiki_link(ws, target));
                }
            });
            backend.set(Some(std::rc::Rc::new(MilkdownBackend::mount(
                element_id.clone(),
                initial,
                on_event,
            ))));
        }
    };

    // Text replaced from outside (reload / revert): push it into the view.
    {
        let mut last_seen = use_signal(String::new);
        use_effect(move || {
            let (text, version) = {
                let d = doc.read();
                (d.text.clone(), d.version)
            };
            let _ = version;
            if !ready() {
                return;
            }
            // Only when the document jumped (not our own keystrokes): the
            // view already has what it emitted.
            if *last_seen.peek() == text {
                return;
            }
            last_seen.set(text.clone());
            if let Some(b) = backend.peek().as_ref() {
                b.set_text(&text);
            }
        });
    }

    let dirty = doc.read().dirty();
    let save = move |_: ()| {
        spawn(async move {
            if let Err(e) = ws.save(node).await {
                error.set(Some(e.to_string()));
            } else {
                error.set(None);
            }
        });
    };
    let reload = move |_| {
        spawn(async move {
            let _ = ws.reload(node).await;
        });
    };

    rsx! {
        document::Stylesheet { href: BUNDLE_CSS }
        document::Stylesheet { href: CSS }
        document::Script { src: BUNDLE, defer: true }
        div {
            class: "mk-rich",
            onkeydown: move |e| {
                if (e.modifiers().ctrl() || e.modifiers().meta()) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    save(());
                }
            },
            div { class: "mk-editor-toolbar",
                span { class: "mk-editor-path", "{doc.read().node.native_key}" }
                if dirty { span { class: "mk-editor-dirty", title: "Unsaved changes", "●" } }
                span { class: "mk-editor-spacer" }
                span { class: "mk-editor-meta", "rich · Ctrl+click a [[link]] to follow it" }
                button { class: "mk-btn", disabled: !dirty, onclick: move |_| save(()), "Save" }
                button { class: "mk-btn", onclick: reload, title: "Discard edits and reload from the source", "Reload" }
            }
            if let Some(e) = error() {
                div { class: "mk-editor-error", "{e}" }
            }
            if !ready() {
                div { class: "mk-rich-loading", "Loading rich editor…" }
            }
            div { id: "{element_id}", class: "mk-rich-host", onmounted: mount }
        }
    }
}

/// `[[Target]]` → the page: the index's resolution first, else `Target.md`
/// by path.
async fn open_wiki_link(mut ws: Workspace, target: String) {
    let candidates = [format!("{target}.md"), target.clone()];
    for c in candidates {
        if let Ok(node) = ws.open_relative_path(&c).await {
            let _ = node;
            return;
        }
    }
    // Ask the index: any node whose label matches (wikilinks::resolve rules
    // live there; phantom pages have no file to open).
    if let Some(index) = ws.index() {
        if let Ok(res) = index
            .source
            .query(moonkale_core::Query::All {
                limit: 5000,
                kinds: Some(vec![moonkale_core::NodeKind::File]),
            })
            .await
        {
            let stem = target.rsplit('/').next().unwrap_or(&target).to_lowercase();
            if let Some(n) = res.nodes.into_iter().find(|n| {
                n.label.to_lowercase() == format!("{stem}.md") || n.label.to_lowercase() == stem
            }) {
                let _ = ws.open_relative_path(&n.native_key).await;
                return;
            }
        }
    }
    ws.set_status(format!("[[{target}]] does not resolve to a page"));
}
