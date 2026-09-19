---
title: "sources-sql — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-sources-sql` (Milestone 2). Design: [[Data Sources]], [[Database Backends]].

- `text.rs` — `classify(sql) → Read | Write | Ddl | Unknown` by first keyword after comments; the read-only gate for every dialect and, later, the LLM policy classifier.
- `sqlite.rs` (feature `sqlite`, `rusqlite` bundled, read-only open flags) — `SqliteSource::open(path)`, id `sqlite:<abs path>`. Schema as a graph: `Database` root → `Table`/view nodes → `Column` nodes (`PRAGMA table_info`), `Contains` edges; `Query::All` returns the whole schema graph. `Query::Text` runs `Read` statements only, capped at 500 rows, values mapped to `core::Value` (`Bytes{len}` for blobs). `Connection` is `!Sync` → `Mutex` + `spawn_blocking`.
- `is_sqlite_path()` (`.sqlite .sqlite3 .db .db3`) is what the Explorer, desktop `open_local` and the server's `open_any` use to decide "database, not folder". Compiles on wasm (only the helpers); the driver is native-only.

Milestone 3: the desktop app now opens `.sqlite` files too (`open_database` in `desktop/src/main.rs`; before only the server did).

Tests: `cargo test -p moonkale-sources-sql --features sqlite` (schema graph; read rows + refused write + SQL error surfaced).

## Milestone 9: DuckDB
`duckdb.rs` (feature `duckdb`, bundled library, native only; desktop and the server enable it): `DuckDbSource::open(path)` opens a `.duckdb` read-only (`AccessMode::ReadOnly`); `open_data_folder(dir)` opens an in-memory database with one view per CSV/TSV/Parquet file directly in `dir` (`read_csv_auto` / `read_parquet`, names sanitised to identifiers, `t_` prefix for leading digits, `_2` on clashes, ≤ 200 files). Schema from `information_schema.tables/columns`; `Query::Text` read statements only (`text::classify`), 500 rows cap; `ValueRef` → `Value` (ints/floats/text/blob; decimals, timestamps and nested types as text). Ids `duckdb:<path>` / `duckdb:<dir>/`. `is_duckdb_path` (`duckdb`, `ddb`) and `is_data_path` (`csv`, `tsv`, `parquet`) live in `lib.rs` for every target (the Explorer uses them). Tests: a data folder with two files and a join, a `.duckdb` file. Bundled build: ~3 minutes extra on a clean release build.
