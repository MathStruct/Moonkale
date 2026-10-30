//! # moonkale-editor-table
//!
//! One panel per opened `Table` node: a SQL box (prefilled with
//! `SELECT * FROM "table" LIMIT 1000`) and a virtualised grid of the result. Runs
//! `Query::Text { dialect: "sql" }` against the node's source; the source
//! decides what is allowed (SQLite is read-only in Milestone 2).
//!
//! The grid is virtualised in rows and columns ([`grid`]), sorts and
//! filters loaded rows or sends typed queries to capable sources. Keyset pages
//! retain authoritative row versions for checked cell edits. Columns resize and
//! a keyboard cursor moves through the visible window.

pub mod extension;
pub mod grid;
pub mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use extension::TableExtension;
pub use panel::TablePanel;

mod edit;

mod browse;
