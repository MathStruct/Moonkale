//! CodeMirror 6 backend over `assets/codemirror.js`.
//!
//! Transport is one `document::eval` per mounted editor: the eval script
//! waits for the bundle, mounts the view, forwards `change` events with
//! `dioxus.send`, and loops on `dioxus.recv` for commands from Rust
//! (`setText`, `focus`, `destroy`). See `packages/js/codemirror/PROTOCOL.md`.
//!
//! The initial text is *sent* over the channel rather than formatted into
//! the script, so it never needs escaping and can be arbitrarily large.

use super::{BackendEvent, CodeEditorBackend};
use dioxus::document::{self, Eval};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

pub const BUNDLE: Asset = asset!("/assets/codemirror.js");

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ToJs<'a> {
    Init { text: &'a str },
    SetText { text: &'a str },
    Focus,
    Undo,
    Redo,
    Destroy,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready,
    Change { text: String },
}

const SCRIPT: &str = r#"
const el = document.getElementById(ELEMENT_ID);
if (!el) { return; }
while (!(window.moonkale && window.moonkale.codemirror)) {
    await new Promise((r) => setTimeout(r, 20));
}
const cm = window.moonkale.codemirror;
const init = await dioxus.recv();
cm.mount(el, init.text, (text) => dioxus.send({ kind: "change", text }));
dioxus.send({ kind: "ready" });
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setText") cm.setText(el, msg.text);
    else if (msg.kind === "focus") cm.focus(el);
    else if (msg.kind === "undo") cm.undo(el);
    else if (msg.kind === "redo") cm.redo(el);
    else if (msg.kind === "destroy") { cm.destroy(el); break; }
}
"#;

pub struct CodeMirrorBackend {
    eval: Eval,
}

impl CodeMirrorBackend {
    pub fn mount(element_id: String, initial: String, on_event: Callback<BackendEvent>) -> Self {
        let script = SCRIPT.replace("ELEMENT_ID", &serde_json::to_string(&element_id).unwrap());
        let eval = document::eval(&script);
        // The JS side blocks on the first recv for the initial text.
        let _ = eval.send(ToJs::Init { text: &initial });

        let mut rx = eval;
        spawn(async move {
            loop {
                match rx.recv::<FromJs>().await {
                    Ok(FromJs::Ready) => on_event.call(BackendEvent::Ready),
                    Ok(FromJs::Change { text }) => on_event.call(BackendEvent::Changed(text)),
                    Err(_) => break, // eval finished or panel unmounted
                }
            }
        });
        Self { eval }
    }
}

impl CodeEditorBackend for CodeMirrorBackend {
    fn set_text(&self, text: &str) {
        let _ = self.eval.send(ToJs::SetText { text });
    }

    fn focus(&self) {
        let _ = self.eval.send(ToJs::Focus);
    }

    fn undo(&self) {
        let _ = self.eval.send(ToJs::Undo);
    }

    fn redo(&self) {
        let _ = self.eval.send(ToJs::Redo);
    }
}

impl Drop for CodeMirrorBackend {
    fn drop(&mut self) {
        let _ = self.eval.send(ToJs::Destroy);
    }
}
