//! # moonkale-sources-sql
//!
//! One `Source` implementation per dialect behind a feature flag. All share
//! [`text`] (statement classification for the read-only gate) and the same
//! lifting: database → `Database` node, tables → `Table` nodes, columns →
//! `Column` nodes, `Contains` edges between them — so a database's *schema*
//! is a graph the graph view can draw.
//!
//! Built: SQLite (`rusqlite`, bundled), DuckDB (+ folders of CSV/TSV/Parquet)
//! and Turso — all read-only: `Query::Text` accepts what [`text`] classifies as
//! a read. Postgres is designed only (vault `architecture/Data Sources.md`).
//! Compiles only on native targets; the `api` server enables the features so
//! web and mobile reach these sources through `RemoteSource`.

pub mod text;

#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
pub mod sqlite;
#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
pub use sqlite::SqliteSource;

#[cfg(all(feature = "duckdb", not(target_arch = "wasm32")))]
pub mod duckdb;
#[cfg(all(feature = "duckdb", not(target_arch = "wasm32")))]
pub use duckdb::DuckDbSource;

/// Files that open as a DuckDB database, and data files a folder exposes as
/// tables (Milestone 9). Path checks only, usable on every target.
pub fn is_duckdb_path(path: &str) -> bool {
    ext_in(path, &["duckdb", "ddb"])
}

pub fn is_data_path(path: &str) -> bool {
    ext_in(path, &["csv", "tsv", "parquet"])
}

fn ext_in(path: &str, list: &[&str]) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| list.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(all(feature = "turso", not(target_arch = "wasm32")))]
pub mod turso;
#[cfg(all(feature = "turso", not(target_arch = "wasm32")))]
pub use turso::TursoSource;

/// A Turso database (Milestone 17). Its files are SQLite files; `*.turso`
/// picks the Turso engine, `.db`/`.sqlite` stay with SQLite. Path check
/// only, usable on every target.
pub fn is_turso_path(path: &str) -> bool {
    ext_in(path, &["turso"])
}

/// File extensions that open as a SQLite database.
pub const SQLITE_EXTENSIONS: &[&str] = &["sqlite", "sqlite3", "db", "db3"];

pub fn is_sqlite_path(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| SQLITE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
