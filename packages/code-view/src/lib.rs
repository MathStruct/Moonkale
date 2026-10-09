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
//!          │                                     ├─ CodeMirror 6 (TS)
//!          └──── setText / focus ───────────►    └─ Rust view (opt-in M1)
//! ```
//!
//! CodeMirror remains the default: [`backend::codemirror`] over `assets/codemirror.js` (built
//! from `packages/js/codemirror`), whole-document changes, Ctrl+S / Save
//! button, dirty marker and conflict reload. The opt-in [`RustCodeEditorPanel`]
//! uses editor-core-owned caret/selection, localized deltas and a virtualized
//! Dioxus view over shared Workspace state. Both surfaces integrate with the
//! shared LSP manager and Workspace navigation. Native Markdown Source supports
//! wiki marks/completion/following, and the native gutter displays presence.
//! Live rust-analyzer protocol and native XWayland keyboard/clipboard/drag checks
//! pass; renderer and remaining platform acceptance stay open.

pub mod backend;
mod edit;
#[doc(hidden)]
pub mod editor_core_spike;
pub mod lsp;
mod native_completion;
mod native_decorations;
mod native_definition;
mod native_diagnostics;
mod native_folding;
mod native_hover;
mod native_indent;
mod native_language;
mod native_layout;
mod uri;
#[cfg(feature = "layout-fixture")]
pub use native_layout::LayoutFixture;
#[cfg(feature = "layout-fixture")]
mod native_browser_geometry;
mod native_lean_folding;
mod native_lsp_tools;
#[cfg(feature = "layout-fixture")]
mod native_markdown;
mod native_model;
mod native_panel;
#[cfg(feature = "layout-fixture")]
mod native_presentation;
#[cfg(feature = "layout-fixture")]
mod native_proportional;
#[cfg(feature = "layout-fixture")]
mod native_proportional_navigation;
#[cfg(feature = "layout-fixture")]
mod native_proportional_run;
#[cfg(feature = "layout-fixture")]
mod native_widgets;
#[cfg(feature = "layout-fixture")]
pub use native_proportional::ProportionalGeometryProbe;
mod native_rename;
mod native_search;
mod native_structure;
mod native_surface;
mod native_wiki;
mod native_workspace_edit;
pub mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use moonkale_ext_api::editor as contract;
pub use native_panel::RustCodeEditorPanel;
pub use panel::{toggle_wrap, CodeEditorPanel};
