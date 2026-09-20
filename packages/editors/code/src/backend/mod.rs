//! `CodeEditorBackend` — the swap point.
//!
//! A backend is mounted into a DOM element the panel renders, shows text,
//! and reports edits. Exactly one backend is compiled in; the panel doesn't
//! know which. The trait is deliberately small for Milestone 1: decorations,
//! selection and capabilities arrive when the index and LSP do.

use dioxus::prelude::*;

pub mod codemirror;

/// Events a backend reports to the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendEvent {
    /// The view is mounted and showing the initial text.
    Ready,
    /// The document changed; the whole text (Milestone 1).
    Changed(String),
    /// The view wants hover text at an LSP position; answer with `hover_result`.
    Hover { id: u32, line: u32, col: u32 },
    /// F12 at a position.
    Definition { line: u32, col: u32 },
    /// The view wants completion proposals; answer with `completion_result`.
    Completion { id: u32, line: u32, col: u32 },
    /// F2: rename the symbol at a position (`word` is what is under the cursor).
    Rename { line: u32, col: u32, word: String },
    /// Ctrl+.: code actions for the selection.
    CodeActions {
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    },
    /// Shift+F12: references of the symbol at a position.
    References { line: u32, col: u32 },
    /// The cursor moved (throttled by the view); presence (Milestone 9).
    Cursor { line: u32, col: u32 },
    /// `[[query` typed (spec 012); answer with `completion_result` — items
    /// whose labels are page targets.
    WikiQuery { id: u32, query: String },
    /// Ctrl/Cmd+click on a `[[link]]`.
    WikiLink { target: String },
}

/// A `[[link]]` span in UTF-16 offsets, for decorations (spec 012).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct WikiMark {
    pub from: u32,
    pub to: u32,
    pub resolved: bool,
}

/// A mounted backend instance. Dropping it tears the view down.
pub trait CodeEditorBackend {
    /// Replace the document text (reload / revert).
    fn set_text(&self, text: &str);
    fn focus(&self);
    fn undo(&self);
    fn redo(&self);
    /// Replace all diagnostics (LSP coordinates).
    fn set_diagnostics(&self, items: &[moonkale_lsp::Diagnostic]);
    fn hover_result(&self, id: u32, text: Option<&str>);
    fn completion_result(&self, id: u32, items: Option<&[moonkale_lsp::CompletionItem]>);
    /// Other people's positions in this document: `(line, label)`.
    fn set_presence(&self, marks: &[(u32, String)]);
    fn set_cursor(&self, line: u32, col: u32);
    /// `[[link]]` spans and whether they resolve (spec 012).
    fn set_wiki_links(&self, marks: &[WikiMark]);
}

/// How the panel mounts a backend. `element_id` is the id of the host `div`;
/// `initial` is the text to show; `on_event` receives backend events.
/// `language` is Rust's id for the document (`Node::language_hint`), which
/// picks the grammar for highlighting (spec 010); `None` = plain text.
pub fn mount(
    element_id: String,
    initial: String,
    language: Option<String>,
    on_event: Callback<BackendEvent>,
) -> Box<dyn CodeEditorBackend> {
    Box::new(codemirror::CodeMirrorBackend::mount(
        element_id, initial, language, on_event,
    ))
}
