//! # moonkale-sources-sql
//!
//! One `Source` implementation per dialect behind a feature flag. All share
//! [`text`] (statement classification for the read-only gate) and the same
//! lifting: database → `Database` node, tables → `Table` nodes, columns →
//! `Column` nodes, `Contains` edges between them — so a database's *schema*
//! is a graph the graph view can draw.
//!
//! **Milestone 2**: SQLite via `rusqlite` (bundled). Read-only: `Query::Text`
//! accepts `SELECT`/`WITH`/`PRAGMA`/`EXPLAIN` only; writes come with the
//! primary-key lifting in `structured`. Compiles only on native targets; the
//! `api` server enables the feature so web/mobile reach it through
//! `RemoteSource`.

pub mod schema;
pub mod structured;
pub mod text;

#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
pub mod sqlite;
#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
pub use sqlite::SqliteSource;

/// File extensions that open as a SQLite database.
pub const SQLITE_EXTENSIONS: &[&str] = &["sqlite", "sqlite3", "db", "db3"];

pub fn is_sqlite_path(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| SQLITE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
