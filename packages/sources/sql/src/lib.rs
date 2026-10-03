//! # moonkale-sources-sql
//!
//! One `Source` implementation per dialect behind a feature flag. All share
//! the read-only gate (`Source::classify`) and the same lifting: database → `Database` node, tables → `Table` nodes, columns →
//! `Column` nodes, `Contains` edges between them — so a database's *schema*
//! is a graph the graph view can draw.
//!
//! Built: SQLite (`rusqlite`, bundled), DuckDB (+ folders of CSV/TSV/Parquet)
//! and Turso — all read-only: `Query::Text` accepts what `Source::classify`
//! (`moonkale_core::source::risk`) calls a read. Postgres is designed only
//! (vault `architecture/Data Sources.md`).
//! Compiles only on native targets; the `api` server enables the features so
//! web and mobile reach these sources through `RemoteSource`.

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

use moonkale_core::source::opener::{OpenFuture, Shape, SourceOpener};
use std::path::PathBuf;

type Open = Option<fn(PathBuf) -> OpenFuture>;

#[cfg(all(feature = "turso", not(target_arch = "wasm32")))]
const TURSO: Open = Some(|p| {
    Box::pin(async move {
        Ok(std::sync::Arc::new(TursoSource::open(p).await?)
            as std::sync::Arc<dyn moonkale_core::Source>)
    })
});
#[cfg(not(all(feature = "turso", not(target_arch = "wasm32"))))]
const TURSO: Open = None;

#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
const SQLITE: Open = Some(|p| {
    Box::pin(async move {
        Ok(std::sync::Arc::new(SqliteSource::open(p)?)
            as std::sync::Arc<dyn moonkale_core::Source>)
    })
});
#[cfg(not(all(feature = "sqlite", not(target_arch = "wasm32"))))]
const SQLITE: Open = None;

#[cfg(all(feature = "duckdb", not(target_arch = "wasm32")))]
const DUCKDB: Open = Some(|p| {
    Box::pin(async move {
        Ok(std::sync::Arc::new(DuckDbSource::open(p)?)
            as std::sync::Arc<dyn moonkale_core::Source>)
    })
});
#[cfg(not(all(feature = "duckdb", not(target_arch = "wasm32"))))]
const DUCKDB: Open = None;

/// A data file opens its *folder* as a DuckDB database of CSV/TSV/Parquet views.
#[cfg(all(feature = "duckdb", not(target_arch = "wasm32")))]
const DATA_FOLDER: Open = Some(|p| {
    Box::pin(async move {
        let dir = p.parent().ok_or(moonkale_core::SourceError::NotFound)?;
        Ok(std::sync::Arc::new(DuckDbSource::open_data_folder(dir)?)
            as std::sync::Arc<dyn moonkale_core::Source>)
    })
});
#[cfg(not(all(feature = "duckdb", not(target_arch = "wasm32"))))]
const DATA_FOLDER: Open = None;

/// The SQL source openers (Milestone 18 phase 2): matched by name on every
/// target, openable where the feature is built.
pub fn openers() -> Vec<SourceOpener> {
    vec![
        SourceOpener {
            id: "turso",
            name: "Turso database",
            shape: Shape::File,
            matches: is_turso_path,
            open: TURSO,
        },
        SourceOpener {
            id: "sqlite",
            name: "SQLite database",
            shape: Shape::File,
            matches: is_sqlite_path,
            open: SQLITE,
        },
        SourceOpener {
            id: "duckdb",
            name: "DuckDB database",
            shape: Shape::File,
            matches: is_duckdb_path,
            open: DUCKDB,
        },
        SourceOpener {
            id: "data-folder",
            name: "CSV/TSV/Parquet folder (DuckDB)",
            shape: Shape::File,
            matches: is_data_path,
            open: DATA_FOLDER,
        },
    ]
}
