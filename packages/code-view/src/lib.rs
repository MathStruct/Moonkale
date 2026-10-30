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
//! shared LSP manager and Workspace navigation. [`graph_rename`] exposes guarded,
//! complete single-file reference rename proposals for opt-in graph controls. Native Markdown Source supports
//! wiki marks/completion/following, and the native gutter displays presence.
//! `editor.markdown_preview` enables bounded inline preview in the Rust Markdown
//! source view (off by default); the `markdown-preview` Cargo feature supplies
//! its geometry/parser independently of the `layout-fixture` test controls.
//! Live rust-analyzer protocol and native XWayland keyboard/clipboard/drag checks
//! pass; renderer and remaining platform acceptance stay open.

pub mod backend;
mod edit;
#[doc(hidden)]
pub mod editor_core_spike;
pub mod graph_rename;
mod group_history;
pub mod lsp;
#[cfg(feature = "markdown-preview")]
mod markdown_guard;
mod native_accessibility;
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
#[cfg(all(feature = "markdown-preview", not(feature = "layout-fixture")))]
pub(crate) use native_layout::LayoutFixture;
#[cfg(feature = "markdown-preview")]
mod native_browser_geometry;
#[cfg(feature = "markdown-preview")]
mod native_doc_comments;
mod native_highlight;
mod native_lean_folding;
mod native_lsp_tools;
#[cfg(feature = "markdown-preview")]
mod native_markdown;
#[cfg(feature = "markdown-preview")]
mod native_markdown_callouts;
#[cfg(feature = "markdown-preview")]
mod native_markdown_lists;
#[cfg(feature = "markdown-preview")]
mod native_markdown_tables;
#[cfg(feature = "markdown-preview")]
mod native_math;
#[cfg(feature = "markdown-preview")]
mod native_math_parser;
mod native_model;
mod native_panel;
#[cfg(feature = "markdown-preview")]
mod native_presentation;
mod native_properties;
#[cfg(feature = "layout-fixture")]
mod native_proportional;
#[cfg(feature = "markdown-preview")]
mod native_proportional_navigation;
#[cfg(feature = "markdown-preview")]
mod native_proportional_run;
#[cfg(feature = "markdown-preview")]
mod native_widgets;
#[cfg(feature = "layout-fixture")]
pub use native_proportional::ProportionalGeometryProbe;
mod native_links;
mod native_remote;
mod native_rename;
mod native_search;
mod native_structure;
mod native_surface;
mod native_wiki;
mod native_wiki_hover;
pub use native_links::ExternalLinkOpener;
#[cfg(feature = "markdown-preview")]
mod native_link_parser;
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

mod native_image_parser;
#[cfg(feature = "markdown-preview")]
mod native_images;

mod preferences;
pub use preferences::{editor_settings, native_commands, run_native_command};

#[cfg(all(test, feature = "markdown-preview"))]
mod markdown_crash_regression {
    use super::*;
    use moonkale_ext_api::editor::DocumentRevision;

    #[test]
    fn fuzz_crash_keeps_source_through_every_public_parser_boundary() {
        for source in markdown_guard::CRASH_INPUTS {
            assert!(native_math_parser::displays(source).is_empty());
            assert!(native_image_parser::previews(source).is_empty());
            assert!(native_markdown_tables::previews(source).is_empty());
            assert!(native_markdown_lists::previews(source).is_empty());
            assert!(native_markdown_callouts::previews(source).is_empty());
            let batch = native_markdown::batch(DocumentRevision(1), source);
            assert_eq!(batch.revision, DocumentRevision(1));
            assert!(batch.values.is_empty());
            assert!(native_properties::front_matter(source).is_none());
            assert!(native_links::heading_position(source, "heading").is_none());
        }
        // An unsuccessful presentation must not poison a retained parser cache.
        assert!(!native_markdown::batch(DocumentRevision(2), "# Good")
            .values
            .is_empty());
    }
}

#[cfg(feature = "markdown-preview")]
pub use native_images::source_image_url;
