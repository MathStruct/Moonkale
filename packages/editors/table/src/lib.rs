//! # moonkale-editor-table
//!
//! Two panels, fully Rust/Dioxus (no JS dependency):
//!
//! - **Query editor**: the code editor with `lang = sql|cypher|typeql` and a
//!   "Run" command that sends a `TextQuery` to the selected source and
//!   opens/refreshes a result grid. Statement classification shows a
//!   warning ribbon on writes.
//! - **Grid**: virtualised rows/columns (only the visible window is in the
//!   DOM), typed cell rendering from `Value`, inline editing that produces
//!   `UpdateProps` ops with optimistic-version checks, sorting/filtering
//!   pushed down to the source when the capability exists.
//!
//! The grid renders any `QueryResult::Rows`, so it is also the "open as
//! table" view for a `GraphView`, a Redis hash, or a CSV in a folder.

pub mod cells;
pub mod edit;
pub mod grid;
pub mod query_panel;
