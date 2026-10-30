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
        key: &'a str,
        epoch: u64,
        sequence: u64,
    },
    SetText {
        text: &'a str,
        epoch: u64,
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
    SetGutterMarks {
        marks: &'a [moonkale_ext_api::workspace::GutterMark],
    },
    SetPresence {
        marks: Vec<PresenceOut<'a>>,
    },
    ShowRange {
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    },
    SetCursor {
        line: u32,
        col: u32,
    },
    SetWikiLinks {
        marks: &'a [super::WikiMark],
    },
    SetPhrases {
        phrases: std::collections::HashMap<String, String>,
    },
    SetWrap {
        wrap: bool,
    },
    SetIndentation {
        insert_spaces: Option<bool>,
        width: Option<u8>,
    },
    Run {
        action: &'a str,
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
    label: String,
    name: &'a str,
    color: usize,
    cursor: Option<&'a moonkale_ext_api::sharing::Cursor>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FromJs {
    Ready,
    Splice {
        changes: Vec<super::Splice>,
        length: u32,
        stamp: crate::group_history::Stamp,
    },
    GroupHistory {
        group: String,
        redo: bool,
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
        anchor: u32,
        head: u32,
    },
    WikiQuery {
        id: u32,
        query: String,
    },
    WikiLink {
        target: String,
    },
    Error {
        message: String,
    },
}

const SCRIPT: &str = r#"
const registrations = window.__mkCodeMirrorMounts ||= new Map();
const owner = Symbol();
registrations.set(ELEMENT_ID, owner);
const init = await dioxus.recv();
let el, cm;
try {
    // The first restored panel can be replaced before a head Script runs.
    // Keep one document-owned import alive across those view lifetimes.
    if (!window.moonkale?.codemirror) {
        const loading = window.__mkCodeMirrorReady ||= import(new URL(BUNDLE_URL, document.baseURI || location.href).href);
        try {
            await Promise.race([loading, new Promise((_, reject) => setTimeout(() => reject(new Error("CodeMirror bundle timed out")), 10000))]);
        } catch (error) {
            if (window.__mkCodeMirrorReady === loading) delete window.__mkCodeMirrorReady;
            throw error;
        }
    }
    cm = window.moonkale?.codemirror;
    if (!cm) throw new Error("CodeMirror bundle did not initialize");
    const deadline = Date.now() + 5000;
    while (!(el = document.getElementById(ELEMENT_ID))) {
        if (Date.now() >= deadline) throw new Error("CodeMirror host did not mount");
        await new Promise(resolve => setTimeout(resolve, 20));
    }
if (registrations.get(ELEMENT_ID) !== owner) return;
el.__mkCodeMirrorOwner = owner;
cm.mount(el, init.text, (changes, length, stamp) => dioxus.send({ kind: "splice", changes, length, stamp }), {
    onHover: (id, line, col) => dioxus.send({ kind: "hover", id, line, col }),
    onDefinition: (line, col) => dioxus.send({ kind: "definition", line, col }),
    onCompletion: (id, line, col) => dioxus.send({ kind: "completion", id, line, col }),
    onRename: (line, col, word) => dioxus.send({ kind: "rename", line, col, word }),
    onCodeActions: (line, col, endLine, endCol) => dioxus.send({ kind: "codeActions", line, col, endLine, endCol }),
    onReferences: (line, col) => dioxus.send({ kind: "references", line, col }),
    onCursor: (line, col, anchor, head) => dioxus.send({ kind: "cursor", line, col, anchor, head }),
    onWikiQuery: (id, query) => dioxus.send({ kind: "wikiQuery", id, query }),
    onWikiLink: (target) => dioxus.send({ kind: "wikiLink", target }),
    sessionKey: init.key,
    epoch: init.epoch,
    sequence: init.sequence,
    onGroupHistory: (group, redo) => dioxus.send({kind:"groupHistory",group,redo}),
    language: init.language || null,
    wrap: !!init.wrap,
});
} catch (e) {
    // A failing mount must never leave "Loading editor…" on screen (spec 016).
    dioxus.send({ kind: "error", message: String(e && e.stack ? e.stack : e) });
    for (;;) { const m = await dioxus.recv(); if (m.kind === "destroy") return; }
}
dioxus.send({ kind: "ready" });
for (;;) {
    const msg = await dioxus.recv();
    if (registrations.get(ELEMENT_ID) !== owner || el.__mkCodeMirrorOwner !== owner) break;
    if (msg.kind === "setText") cm.setText(el, msg.text, msg.epoch);
    else if (msg.kind === "focus") cm.focus(el);
    else if (msg.kind === "undo") cm.undo(el);
    else if (msg.kind === "redo") cm.redo(el);
    else if (msg.kind === "diagnostics") cm.setLspDiagnostics(el, msg.items);
    else if (msg.kind === "hoverResult") cm.hoverResult(el, msg.id, msg.text);
    else if (msg.kind === "completionResult") cm.completionResult(el, msg.id, msg.items);
    else if (msg.kind === "setGutterMarks") cm.setGutterMarks(el, msg.marks);
    else if (msg.kind === "setPresence") cm.setPresence(el, msg.marks);
    else if (msg.kind === "showRange") cm.showRange(el, msg.line, msg.col, msg.end_line, msg.end_col);
    else if (msg.kind === "setCursor") cm.setCursor(el, msg.line, msg.col);
    else if (msg.kind === "setWikiLinks") cm.setWikiLinks(el, msg.marks);
    else if (msg.kind === "setPhrases") cm.setPhrases(el, msg.phrases);
    else if (msg.kind === "setWrap") cm.setWrap(el, msg.wrap);
    else if (msg.kind === "setIndentation") cm.setIndentation(el, msg.insert_spaces, msg.width);
    else if (msg.kind === "run") cm.run(el, msg.action);
    else if (msg.kind === "destroy") { cm.destroy(el); registrations.delete(ELEMENT_ID); delete el.__mkCodeMirrorOwner; break; }
}
"#;

pub struct CodeMirrorBackend {
    eval: Eval,
    binding: crate::group_history::Binding,
}

impl CodeMirrorBackend {
    pub fn mount(
        element_id: String,
        initial: String,
        language: Option<String>,
        wrap: bool,
        on_event: Callback<BackendEvent>,
    ) -> Self {
        Self::mount_bound(
            element_id,
            initial,
            language,
            wrap,
            on_event,
            crate::group_history::Binding::standalone(),
        )
    }
    pub(crate) fn mount_bound(
        element_id: String,
        initial: String,
        language: Option<String>,
        wrap: bool,
        on_event: Callback<BackendEvent>,
        binding: crate::group_history::Binding,
    ) -> Self {
        let script = SCRIPT
            .replace("ELEMENT_ID", &serde_json::to_string(&element_id).unwrap())
            .replace(
                "BUNDLE_URL",
                &serde_json::to_string(&BUNDLE.to_string()).unwrap(),
            );
        let eval = document::eval(&script);
        // The JS side blocks on the first recv for the initial text.
        let _ = eval.send(ToJs::Init {
            text: &initial,
            language: language.as_deref(),
            wrap,
            key: &binding.key,
            epoch: binding.stamp.get().epoch,
            sequence: binding.stamp.get().sequence,
        });

        let mut rx = eval;
        moonkale_ext_api::crash::spawn(async move {
            loop {
                match rx.recv::<FromJs>().await {
                    Ok(FromJs::Ready) => on_event.call(BackendEvent::Ready),
                    Ok(FromJs::Splice {
                        changes,
                        length,
                        stamp,
                    }) => on_event.call(BackendEvent::Spliced {
                        changes,
                        length,
                        stamp,
                    }),
                    Ok(FromJs::GroupHistory { group, redo }) => {
                        if let Ok(group) = group.parse() {
                            on_event.call(BackendEvent::GroupHistory { group, redo });
                        }
                    }
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
                    Ok(FromJs::Cursor {
                        line,
                        col,
                        anchor,
                        head,
                    }) => on_event.call(BackendEvent::Cursor {
                        line,
                        col,
                        anchor,
                        head,
                    }),
                    Ok(FromJs::WikiQuery { id, query }) => {
                        on_event.call(BackendEvent::WikiQuery { id, query })
                    }
                    Ok(FromJs::WikiLink { target }) => {
                        on_event.call(BackendEvent::WikiLink { target })
                    }
                    Ok(FromJs::Error { message }) => {
                        tracing::error!("codemirror: mount failed: {message}");
                        on_event.call(BackendEvent::Failed(message));
                    }
                    Err(dioxus::document::EvalError::Serialization(e)) => {
                        tracing::warn!("codemirror bridge: unreadable message: {e}");
                    }
                    Err(_) => break, // eval finished or panel unmounted
                }
            }
        });
        Self { eval, binding }
    }
}

impl CodeEditorBackend for CodeMirrorBackend {
    fn set_text(&self, text: &str) {
        // Group cancellation flushes canonical changes after the held commit.
        if self.binding.held.borrow().is_some() {
            return;
        }
        let mut stamp = self.binding.stamp.get();
        stamp.epoch = stamp.epoch.saturating_add(1);
        stamp.sequence = 0;
        self.binding.stamp.set(stamp);
        let _ = self.eval.send(ToJs::SetText {
            text,
            epoch: stamp.epoch,
        });
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

    fn set_gutter_marks(&self, marks: &[moonkale_ext_api::workspace::GutterMark]) {
        let _ = self.eval.send(ToJs::SetGutterMarks { marks });
    }

    fn set_presence(&self, marks: &[moonkale_ext_api::presence::Member]) {
        let marks = marks
            .iter()
            .filter_map(|member| {
                Some(PresenceOut {
                    line: member.line?,
                    label: member.initials(),
                    name: &member.name,
                    color: member.color(),
                    cursor: member.sharing.as_ref().and_then(|p| p.cursor.as_ref()),
                })
            })
            .collect();
        let _ = self.eval.send(ToJs::SetPresence { marks });
    }

    fn show_range(&self, line: u32, col: u32, end_line: u32, end_col: u32) {
        let _ = self.eval.send(ToJs::ShowRange {
            line,
            col,
            end_line,
            end_col,
        });
    }

    fn set_cursor(&self, line: u32, col: u32) {
        let _ = self.eval.send(ToJs::SetCursor { line, col });
    }

    fn set_wiki_links(&self, marks: &[super::WikiMark]) {
        let _ = self.eval.send(ToJs::SetWikiLinks { marks });
    }

    fn set_phrases(&self, phrases: std::collections::HashMap<String, String>) {
        let _ = self.eval.send(ToJs::SetPhrases { phrases });
    }
    fn set_wrap(&self, wrap: bool) {
        let _ = self.eval.send(ToJs::SetWrap { wrap });
    }

    fn set_indentation(&self, insert_spaces: Option<bool>, width: Option<u8>) {
        let _ = self.eval.send(ToJs::SetIndentation {
            insert_spaces,
            width,
        });
    }

    fn run(&self, action: moonkale_ext_api::EditorAction) {
        use moonkale_ext_api::EditorAction as A;
        let action = match action {
            A::NativeReload
            | A::NativeHover
            | A::NativeComplete
            | A::NativePreview
            | A::NativeDocPreview
            | A::NativeCodeMirror => return,
            A::Find => "find",
            A::Replace => "replace",
            A::Rename => "rename",
            A::CodeActions => "codeActions",
            A::Definition => "definition",
            A::References => "references",
            A::ToggleComment => "toggleComment",
            A::FoldAll => "foldAll",
            A::UnfoldAll => "unfoldAll",
        };
        let _ = self.eval.send(ToJs::Run { action });
    }
}

impl Drop for CodeMirrorBackend {
    fn drop(&mut self) {
        let _ = self.eval.send(ToJs::Destroy);
    }
}
