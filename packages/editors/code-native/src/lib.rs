//! # moonkale-editor-code-native
//!
//! The second code editor ([[Code Editor Implementations]], Milestone 14):
//! `dioxus-code-editor` — a textarea over a tree-sitter-highlighted layer,
//! all Rust (the grammars are `arborium`, tree-sitter compiled to Rust and
//! wasm), so it runs on desktop, web and phone without a JavaScript
//! bundle — behind the same `Document` the CodeMirror panel edits. Opt-in;
//! `editor.implementation` and the toolbar switch pick per document.

mod panel;

pub use panel::{NativeCodeExtension, PANEL_PREFIX};
