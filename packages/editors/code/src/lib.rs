//! # moonkale-editor-code
//!
//! The code editing "window", registered as a static extension that
//! contributes one closable panel per open document. The editor itself —
//! the CodeMirror component, its backend and the LSP sessions — is
//! `moonkale-code-view` (Milestone 18 phase 4.1), which Markdown's Source
//! mode uses too.

pub mod extension;

pub use extension::CodeEditorExtension;
pub use moonkale_code_view::{backend, lsp, panel, toggle_wrap, CodeEditorPanel};
