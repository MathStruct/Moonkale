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
    Diagnostics { items: Vec<DiagOut<'a>> },
    HoverResult { id: u32, text: Option<&'a str> },
    SetCursor { line: u32, col: u32 },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagOut<'a> {
    line: u32,
    col: u32,
    end_line: u32,
    end_col: u32,
    severity: &'a str,
    message: &'a str,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready,
    Change { text: String },
    Hover { id: u32, line: u32, col: u32 },
    Definition { line: u32, col: u32 },
}

const SCRIPT: &str = r#"
const el = document.getElementById(ELEMENT_ID);
if (!el) { return; }
while (!(window.moonkale && window.moonkale.codemirror)) {
    await new Promise((r) => setTimeout(r, 20));
}
const cm = window.moonkale.codemirror;
const init = await dioxus.recv();
cm.mount(el, init.text, (text) => dioxus.send({ kind: "change", text }), {
    onHover: (id, line, col) => dioxus.send({ kind: "hover", id, line, col }),
    onDefinition: (line, col) => dioxus.send({ kind: "definition", line, col }),
});
dioxus.send({ kind: "ready" });
for (;;) {
    const msg = await dioxus.recv();
    if (msg.kind === "setText") cm.setText(el, msg.text);
    else if (msg.kind === "focus") cm.focus(el);
    else if (msg.kind === "undo") cm.undo(el);
    else if (msg.kind === "redo") cm.redo(el);
    else if (msg.kind === "diagnostics") cm.setLspDiagnostics(el, msg.items);
    else if (msg.kind === "hoverResult") cm.hoverResult(el, msg.id, msg.text);
    else if (msg.kind === "setCursor") cm.setCursor(el, msg.line, msg.col);
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
                    Ok(FromJs::Hover { id, line, col }) => {
                        on_event.call(BackendEvent::Hover { id, line, col })
                    }
                    Ok(FromJs::Definition { line, col }) => {
                        on_event.call(BackendEvent::Definition { line, col })
                    }
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

    fn set_diagnostics(&self, items: &[moonkale_lsp::Diagnostic]) {
        let items = items
            .iter()
            .map(|d| DiagOut {
                line: d.line,
                col: d.col,
                end_line: d.end_line,
                end_col: d.end_col,
                severity: d.severity,
                message: &d.message,
            })
            .collect();
        let _ = self.eval.send(ToJs::Diagnostics { items });
    }

    fn hover_result(&self, id: u32, text: Option<&str>) {
        let _ = self.eval.send(ToJs::HoverResult { id, text });
    }

    fn set_cursor(&self, line: u32, col: u32) {
        let _ = self.eval.send(ToJs::SetCursor { line, col });
    }
}

impl Drop for CodeMirrorBackend {
    fn drop(&mut self) {
        let _ = self.eval.send(ToJs::Destroy);
    }
}
