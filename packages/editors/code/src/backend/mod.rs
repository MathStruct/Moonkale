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
}

/// A mounted backend instance. Dropping it tears the view down.
pub trait CodeEditorBackend {
    /// Replace the document text (reload / revert).
    fn set_text(&self, text: &str);
    fn focus(&self);
    fn undo(&self);
    fn redo(&self);
}

/// How the panel mounts a backend. `element_id` is the id of the host `div`;
/// `initial` is the text to show; `on_event` receives backend events.
pub fn mount(
    element_id: String,
    initial: String,
    on_event: Callback<BackendEvent>,
) -> Box<dyn CodeEditorBackend> {
    Box::new(codemirror::CodeMirrorBackend::mount(
        element_id, initial, on_event,
    ))
}
