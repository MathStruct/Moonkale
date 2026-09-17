---
tags: [research, sources]
---
# Database Backends — driver survey (2026-09-17)

Verified on crates.io at time of writing. Re-verify before implementation.

| Target | Crate | Version | Maturity | Notes |
|---|---|---|---|---|
| Postgres / Supabase | `sqlx` (postgres) | 0.9 | high | async, pure Rust; `pgvector` via `pgvector` crate. Supabase REST is a browser-capable alternative (deferred). |
| SQLite | `sqlx` (sqlite) / `rusqlite` | 0.9 / — | high | `rusqlite` if we need extensions (`sqlite-vec`). |
| DuckDB | `duckdb` | 1.x | high | bundled build; columnar; single writer. Also powers "folder of CSV/parquet as tables". |
| Turso / libSQL | `libsql` | 0.10-pre | medium | embedded replicas + remote. |
| TypeDB | `typedb-driver` | 3.12 | high (official) | TypeQL; strongly typed schema. |
| LadybugDB | **`lbug`** | 0.20.4 | high (official, [docs.rs](https://docs.rs/crate/lbug/latest)) | Formerly Kuzu. Embedded, Cypher, FTS + vector indices. Build: downloads a prebuilt static `liblbug`, else compiles C++ via cmake (`LBUG_SHARED`, `LBUG_LIBRARY_DIR`, `LBUG_INCLUDE_DIR`, `LBUG_BUILD_FROM_SOURCE`). Extensions need extra linker flags in binaries. Sync API → `spawn_blocking`. Also has Wasm bindings (browser-direct embedded graph DB is conceivable later). Best first graph backend. |
| HelixDB | **`helix-db`** (`features = ["embedded"]`) | 3.0 | medium | Two modes: **embedded** (`Client::open(HelixDbSource::Disk{root,database}).await`; also Memory / S3-compatible) and **server** (`helix init local --name dev && helix start dev` → `POST http://localhost:6969/v2/query`, operation-tree body). Same request contract in both. Server mode is plain HTTP → usable from the *browser build* directly. Graph+vector in one — candidate RAG index backend. (`helix-rs` 0.0.0 is a dead placeholder.) |
| FalkorDB | `falkordb` | 0.10 | medium (official) | RESP protocol; Cypher. Shares plumbing with Redis. |
| Redis / Dragonfly | `redis` | 1.7 | high | keyspace notifications for events. |

## Implications
- Embedded engines (SQLite, DuckDB, Ladybug) give a **no-server local-first** experience — implement first.
- All four graph DBs now have usable Rust paths; `lbug` and `helix-db` embedded both compile native C/C++ or storage engines, so keep them behind features to protect build times.
- HelixDB server mode is the first candidate for a **browser-direct** source (no `api` proxy needed).
- All are native-only → [[ADR-0006 Native drivers only on native targets]].

## Also considered
- `lancedb` 0.38 / `usearch` 2.26 for vectors ([[Indexing]]).
- `tantivy` 0.26 for full-text.
- `recoco` 0.2 (Rust-only CocoIndex fork) for the indexing pipeline.
