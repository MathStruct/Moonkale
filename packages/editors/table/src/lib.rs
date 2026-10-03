//! # moonkale-editor-table
//!
//! One panel per opened `Table` node: a SQL box (prefilled with
//! `SELECT * FROM "table" LIMIT 200`) and a grid of the result. Runs
//! `Query::Text { dialect: "sql" }` against the node's source; the source
//! decides what is allowed (SQLite is read-only in Milestone 2).
//!
//! Not yet: virtualised rows, pushdown sort/filter, cell editing (needs the
//! primary-key lifting), "open as table" for graph views.

pub mod extension;
pub mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use extension::TableExtension;
pub use panel::TablePanel;
