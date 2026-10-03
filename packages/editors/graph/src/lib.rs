//! # moonkale-editor-graph
//!
//! The graph "window". This crate is the *host*: a Dioxus panel that queries
//! the index, sends the subgraph as JSON to the renderer module
//! (`moonkale-graph-render`, a standalone wasm asset built into `assets/`),
//! and turns the module's hover/click events into popups and editor opens.
//!
//! The renderer runs inside the page/webview on every platform (ADR-0011
//! plan A). The host never touches the canvas itself.
//!
//! Milestone 2: whole-graph and local-graph modes, kind filters, hover
//! popup, double-click to open, backend/counts readout. Layouts beyond
//! force-directed, styling contributions, 3D and GPU layout are later phases
//! (vault `editors/Graph View.md`).

pub mod extension;
pub mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use extension::GraphExtension;
pub use panel::GraphPanel;
