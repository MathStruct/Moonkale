//! # moonkale-code-view
//!
//! The code editing component: [`CodeEditorPanel`] over a swappable
//! backend, and the LSP sessions it talks to ([`lsp::LspManager`]). Used by
//! the Code editor extension (`moonkale-editor-code`) and by Markdown's
//! Source mode; split out of the Code extension in Milestone 18 phase 4.1 so
//! that no extension depends on another.
//!
//! **The document lives in Rust** (`moonkale_ext_api::Document`, owned by the
//! `Workspace`). The visual editor is a *view* that receives text and sends
//! back edits. That is the seam that makes the backend swappable:
//!
//! ```text
//!   Document (Rust, truth) ◄──── change ────  Backend view
//!          │                                     ├─ CodeMirror 6 (TS, today)
//!          └──── setText / focus ───────────►    └─ Rust-native (later)
//! ```
//!
//! Milestone 1: [`backend::codemirror`] over `assets/codemirror.js` (built
//! from `packages/js/codemirror`), whole-document changes, Ctrl+S / Save
//! button, dirty marker, conflict → reload. No highlighting or LSP yet —
//! those arrive as *decorations* pushed from the index, never as language
//! packages inside the bundle.

pub mod backend;
pub mod lsp;
pub mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use panel::{toggle_wrap, CodeEditorPanel};
