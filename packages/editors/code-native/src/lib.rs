//! # moonkale-editor-code-native
//!
//! Opt-in adapter to `moonkale-code-view`: editor-core owns caret/selection,
//! Dioxus renders viewport rows, and Rust tree-sitter supplies highlighting.
//! Workspace owns canonical text and revisioned history. Desktop clipboard
//! is enabled by the desktop app; browser input uses Dioxus events.

mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use panel::{NativeCodeExtension, PANEL_PREFIX};
