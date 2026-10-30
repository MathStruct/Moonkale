---
title: "sources-sql — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-sources-sql` (Milestone 2). Design: [[Data Sources]], [[Database Backends]].

- `text.rs` — `classify(sql) → Read | Write | Ddl | Unknown` by first keyword after comments; the read-only gate for every dialect and, later, the LLM policy classifier.
- `sqlite.rs` (feature `sqlite`, `rusqlite` bundled, read/write open with read-only fallback) — `SqliteSource::open(path)`, id `sqlite:<abs path>`. Schema as a graph: `Database` root → `Table`/view nodes → `Column` nodes (`PRAGMA table_info`), `Contains` edges; `Query::All` returns the whole schema graph. `Query::Text` runs `Read` statements only, capped at 50 000 rows, values mapped to `core::Value` (`Bytes{len}` for blobs). `Connection` is `!Sync` → `Mutex` + `spawn_blocking`.
- `is_sqlite_path()` (`.sqlite .sqlite3 .db .db3`) is what the Explorer, desktop `open_local` and the server's `open_any` use to decide "database, not folder". Compiles on wasm (only the helpers); the driver is native-only.

Milestone 3: the desktop app now opens `.sqlite` files too (`open_database` in `desktop/src/main.rs`; before only the server did).

Tests: `cargo test -p moonkale-sources-sql --features sqlite` (schema graph; read rows + refused write + SQL error surfaced).

## Milestone 9: DuckDB
`duckdb.rs` (feature `duckdb`, bundled library, native only; desktop and the server enable it): `DuckDbSource::open(path)` opens a `.duckdb` for typed row writes, with read-only fallback; `open_data_folder(dir)` opens an in-memory database with one view per CSV/TSV/Parquet file directly in `dir` (`read_csv_auto` / `read_parquet`, names sanitised to identifiers, `t_` prefix for leading digits, `_2` on clashes, ≤ 200 files). Schema from `information_schema.tables/columns`; `Query::Text` read statements only (`text::classify`), 50 000 rows cap; `ValueRef` → `Value` (ints/floats/text/blob; decimals, timestamps and nested types as text). Ids `duckdb:<path>` / `duckdb:<dir>/`. `is_duckdb_path` (`duckdb`, `ddb`) and `is_data_path` (`csv`, `tsv`, `parquet`) live in `lib.rs` for every target (the Explorer uses them). Tests: a data folder with two files and a join, a `.duckdb` file. Bundled build: ~3 minutes extra on a clean release build.

## Milestone 17: Turso
`turso.rs` (feature `turso`, `turso` 0.7, beta; native only; desktop and the server enable it): `TursoSource::open(path)` (async) opens an existing `*.turso` file — a SQLite-format file, but `.db`/`.sqlite` keep opening with SQLite, so the engine is chosen by extension. Same lifting as SQLite (`sqlite_schema`, `PRAGMA table_info`), `Query::Text` read statements only through `text::classify`, 50 000 rows cap, `turso::Value` → `Value`. Ids `turso:<path>`. `is_turso_path` in `lib.rs` for every target. Test `tests/turso.rs` (a database written through Turso itself: tables and a view, columns, a read, a refused write, the schema graph). Note: `turso_core` brings `simsimd` and, with the default `fts` feature, tantivy → `zstd-sys`; both clash with LadybugDB's static library (P-144).


## Typed row writes (WP-14, 2026-10-30)

`rows.rs` shares schema validation, prepared statement planning, exact typed-value
BLAKE3 row versions and transaction handling. SQLite/Turso use a primary key or
an unshadowed rowid alias. DuckDB requires a primary key; its physical rowid is
not a durable identity. Native values remain separate from displayed DATE projections: versions hash the complete native row, keys retain exact stored text, and `RowSnapshot::stored` retains differing native values for exact undo. Keys and columns must match current main-schema metadata;
views, virtual tables, generated columns and unusable keys refuse with a reason.
Attachments cannot be named as write targets. Unsupported/lossy cell projections
(including blob bytes represented only by size) refuse row editing rather than
issuing an incomplete concurrency token. Numeric vectors and dates are supported
where they have lossless typed representations.

One `RowBatch` (1–64 operations, up to 4 MiB) uses one transaction. Versions are
checked inside it before each write; one bad operation rolls back the entire
batch. Insert requires a complete explicit key and asserts absence. Primary-key
changes use delete plus insert. Preview validates every base and prepares/binds EXPLAIN statements without executing mutations or defaults, then rolls back its validation transaction. Apply repeats that preflight before the first mutation and checks database constraints during execution. Duplicate operations on one row must be combined. SQL query text remains read-only. SQLite also checks the
prepared statement's readonly flag. DuckDB raw SQL uses read-only file connections, including stateful SELECT functions such as `nextval`; vector values bind as numeric JSON text for the target column to cast because the driver cannot bind list parameters. DuckDB retains disabled external access,
autoload/install restrictions and locked configuration. File-mode connections
last one operation so external writers can work while the source is idle; a
batch retains its connection through commit. Data-folder views remain read-only.
Turso enables generated-column parsing to inspect/refuse such tables.

`tests/rows.rs` runs the same update/insert/delete, stale, rollback, identifier
and value injection, view and generated-column acceptance against each engine.
`change_row` is a disposable browser-fixture writer with one fixed typed update;
it is not an application SQL command interface.

Database file paths must have lossless UTF-8 source identities; ambiguous non-UTF-8 paths refuse before opening a driver or creating a lossy Turso path.


## Table pushdown (WP-13, 2026-10-30)

All three engines share the `rows.rs` prepared planner for `TableQuery`. It
validates request bounds before metadata access, verifies identifiers against
fresh main-schema columns/keys, binds filter/cursor/limit values and seeks
lexicographically without OFFSET. Explicit NULLS LAST applies in both directions;
the complete native row key breaks ties. CASE-projected native values produce
cursors, preserving SQLite DATE text rather than comparing localized display.
`read_rows` delegates to the same stable key order. Read-only tables with usable
keys can page; unusable/unsupported identities keep the existing refusal/fallback.
`tests/table_query.rs` compares complete small-table traversals against a
brute-force reference in SQLite, DuckDB and Turso. `seed_scale` creates a new
million-row DuckDB fixture for the browser suite, never a committed database.
