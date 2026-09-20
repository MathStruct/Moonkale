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
    Init {
        text: &'a str,
        language: Option<&'a str>,
        wrap: bool,
    },
    SetText {
        text: &'a str,
    },
    Focus,
    Undo,
    Redo,
    Destroy,
    Diagnostics {
        items: Vec<DiagOut<'a>>,
    },
    HoverResult {
        id: u32,
        text: Option<&'a str>,
    },
    CompletionResult {
        id: u32,
        items: Option<&'a [moonkale_lsp::CompletionItem]>,
    },
    SetPresence {
        marks: Vec<PresenceOut<'a>>,
    },
    SetCursor {
        line: u32,
        col: u32,
    },
    SetWikiLinks {
        marks: &'a [super::WikiMark],
    },
    SetWrap {
        wrap: bool,
    },
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

#[derive(Serialize)]
struct PresenceOut<'a> {
    line: u32,
    label: &'a str,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready,
    Change {
        text: String,
    },
    Hover {
        id: u32,
        line: u32,
        col: u32,
    },
    Definition {
        line: u32,
        col: u32,
    },
    Completion {
        id: u32,
        line: u32,
        col: u32,
    },
    Rename {
        line: u32,
        col: u32,
        word: String,
    },
    #[serde(rename_all = "camelCase")]
    CodeActions {
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    },
    References {
        line: u32,
        col: u32,
    },
    Cursor {
        line: u32,
        col: u32,
    },
    WikiQuery {
        id: u32,
        query: String,
    },
    WikiLink {
        target: String,
    },
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
    onCompletion: (id, line, col) => dioxus.send({ kind: "completion", id, line, col }),
    onRename: (line, col, word) => dioxus.send({ kind: "rename", line, col, word }),
    onCodeActions: (line, col, endLine, endCol) => dioxus.send({ kind: "codeActions", line, col, endLine, endCol }),
    onReferences: (line, col) => dioxus.send({ kind: "references", line, col }),
    onCursor: (line, col) => dioxus.send({ kind: "cursor", line, col }),
    onWikiQuery: (id, query) => dioxus.send({ kind: "wikiQuery", id, query }),
    onWikiLink: (target) => dioxus.send({ kind: "wikiLink", target }),
    language: init.language || null,
    wrap: !!init.wrap,
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
    else if (msg.kind === "completionResult") cm.completionResult(el, msg.id, msg.items);
    else if (msg.kind === "setPresence") cm.setPresence(el, msg.marks);
    else if (msg.kind === "setCursor") cm.setCursor(el, msg.line, msg.col);
    else if (msg.kind === "setWikiLinks") cm.setWikiLinks(el, msg.marks);
    else if (msg.kind === "setWrap") cm.setWrap(el, msg.wrap);
    else if (msg.kind === "destroy") { cm.destroy(el); break; }
}
"#;

pub struct CodeMirrorBackend {
    eval: Eval,
}

impl CodeMirrorBackend {
    pub fn mount(
        element_id: String,
        initial: String,
        language: Option<String>,
        wrap: bool,
        on_event: Callback<BackendEvent>,
    ) -> Self {
        let script = SCRIPT.replace("ELEMENT_ID", &serde_json::to_string(&element_id).unwrap());
        let eval = document::eval(&script);
        // The JS side blocks on the first recv for the initial text.
        let _ = eval.send(ToJs::Init {
            text: &initial,
            language: language.as_deref(),
            wrap,
        });

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
                    Ok(FromJs::Completion { id, line, col }) => {
                        on_event.call(BackendEvent::Completion { id, line, col })
                    }
                    Ok(FromJs::Rename { line, col, word }) => {
                        on_event.call(BackendEvent::Rename { line, col, word })
                    }
                    Ok(FromJs::CodeActions {
                        line,
                        col,
                        end_line,
                        end_col,
                    }) => on_event.call(BackendEvent::CodeActions {
                        line,
                        col,
                        end_line,
                        end_col,
                    }),
                    Ok(FromJs::References { line, col }) => {
                        on_event.call(BackendEvent::References { line, col })
                    }
                    Ok(FromJs::Cursor { line, col }) => {
                        on_event.call(BackendEvent::Cursor { line, col })
                    }
                    Ok(FromJs::WikiQuery { id, query }) => {
                        on_event.call(BackendEvent::WikiQuery { id, query })
                    }
                    Ok(FromJs::WikiLink { target }) => {
                        on_event.call(BackendEvent::WikiLink { target })
                    }
                    Err(dioxus::document::EvalError::Serialization(e)) => {
                        tracing::warn!("codemirror bridge: unreadable message: {e}");
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

    fn completion_result(&self, id: u32, items: Option<&[moonkale_lsp::CompletionItem]>) {
        let _ = self.eval.send(ToJs::CompletionResult { id, items });
    }

    fn set_presence(&self, marks: &[(u32, String)]) {
        let marks = marks
            .iter()
            .map(|(line, label)| PresenceOut { line: *line, label })
            .collect();
        let _ = self.eval.send(ToJs::SetPresence { marks });
    }

    fn set_cursor(&self, line: u32, col: u32) {
        let _ = self.eval.send(ToJs::SetCursor { line, col });
    }

    fn set_wiki_links(&self, marks: &[super::WikiMark]) {
        let _ = self.eval.send(ToJs::SetWikiLinks { marks });
    }

    fn set_wrap(&self, wrap: bool) {
        let _ = self.eval.send(ToJs::SetWrap { wrap });
    }
}

impl Drop for CodeMirrorBackend {
    fn drop(&mut self) {
        let _ = self.eval.send(ToJs::Destroy);
    }
}
