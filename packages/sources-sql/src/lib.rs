//! # moonkale-sources-sql
//!
//! One `Source` implementation per dialect behind a feature flag. All share:
//! - [`schema`]: introspection into `core::source::descriptor::Schema`,
//! - [`text`]: pass-through of `TextQuery` with streaming rows,
//! - [`structured`]: translation of the structured `Query` IR to SQL.
//!
//! Compiles only on native targets — the drivers link C libraries. The
//! `api` crate enables these features on the server so web/mobile users get
//! them through `moonkale-sources::remote`.
//!
//! Dialect notes (see vault `research/Database Backends.md`):
//! - **Postgres/Supabase**: `sqlx`. Supabase additionally exposes REST/RPC
//!   which *would* work from a browser — deferred; treat as Postgres.
//! - **SQLite**: `sqlx` sqlite feature (async) — `rusqlite` if we need
//!   extensions like `sqlite-vec`.
//! - **DuckDB**: `duckdb` crate; columnar, great for the "open a folder of
//!   CSV/parquet" use case; single-writer — surface that in capabilities.
//! - **Turso**: `libsql` for both embedded replicas and remote.

#[cfg(feature = "duckdb")]
pub mod duckdb;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod schema;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod structured;
pub mod text;
#[cfg(feature = "turso")]
pub mod turso;
