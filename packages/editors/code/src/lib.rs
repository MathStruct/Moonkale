//! # moonkale-editor-code
//!
//! The code editing "window". Registered as a static extension contributing
//! an `Editor` for `NodeKind::File` (any text) with priority below the
//! specialised editors.
//!
//! **The document lives in Rust.** The editor keeps a `ropey::Rope` per open
//! node, applies `core::Transaction` patches to it, and treats the visual
//! editor as a *view* that receives the text and sends back edits. That is
//! the seam that makes the backend swappable:
//!
//! ```text
//!   Rope (Rust, truth) ◄──── edits ────  Backend view
//!        │                                  ├─ CodeMirror 6 (TS, today)
//!        └──── text/decorations ───────►    └─ Rust-native (tomorrow: a Dioxus
//!                                               virtualised text view or a
//!                                               wgpu text renderer)
//! ```
//!
//! Highlighting, folding, and structure come from tree-sitter in
//! `moonkale-index` and are *pushed* to the backend as decorations, so a
//! backend needs no language knowledge of its own. LSP features come through
//! `moonkale-lsp::features` in neutral types. This means the CodeMirror
//! bundle is small (core + view + minimal keymap), and replacing it does not
//! touch languages, LSP, or the document model.
//!
//! See vault: `editors/Code Editor.md`, `architecture/JS Interop Boundary.md`.

pub mod backend;
pub mod decorations;
pub mod document;
pub mod languages;
pub mod panel;
