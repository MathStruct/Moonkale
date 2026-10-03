//! # moonkale-editor-code-native
//!
//! The second code editor ([[Code Editor Implementations]], Milestone 14):
//! `dioxus-code-editor` — a textarea over a tree-sitter-highlighted layer,
//! all Rust (the grammars are `arborium`, tree-sitter compiled to Rust and
//! wasm), so it runs on desktop, web and phone without a JavaScript
//! bundle — behind the same `Document` the CodeMirror panel edits. Opt-in;
//! `editor.implementation` and the toolbar switch pick per document.

mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use panel::{NativeCodeExtension, PANEL_PREFIX};
