//! # moonkale-editor-code
//!
//! The code editing "window", registered as a static extension that
//! contributes one closable panel per open document.
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
pub mod extension;
pub mod panel;

pub use extension::CodeEditorExtension;
pub use panel::CodeEditorPanel;
