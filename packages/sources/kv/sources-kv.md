---
title: "sources-kv — implementation notes"
tags: [crate-notes, milestone-17]
---
Notes for `moonkale-sources-kv` (Milestone 17). Design: [[Data Sources]]. Log: [[Milestone 17 - Implementation Log]].

- `kv.rs` — what every embedded key/value store shares: `KvStore` (`tables`, `scan(table, prefix, limit)`, `get`), `KvTable` (name, key/value type names, count, readable), the **`kv` dialect** (`tables`, `scan <table> [prefix <p>] [limit <n>]`, `get <table> <key>`; quoted words, `0x…` hex keys; ≤ 5000 rows) and `show_bytes` (UTF-8 without control characters as text, else `0x…` hex, capped).
- `source.rs` — `KvSource<S: KvStore>`: ids `<kind>:<path>`, the database → tables tree (`NodeKind::Table`, label `name · count`), `Query::All` database → tables, `Query::Text` in `kv` → a `key`/`value` table. Read-only (`apply` refuses). Blocking store calls on `spawn_blocking`.
- `redb_store.rs` (feature `redb`, `redb` 4): `ReadOnlyDatabase`; each table is probed with `&[u8] → &[u8]`, and `TableTypeMismatch` names the stored types; a macro dispatches the name pair to the matching Rust types (keys `&str String &[u8] u64 i64 u32 i32 u128`, values those plus `Vec<u8> f64 bool ()`). Strings and bytes are shown by their bytes, the rest by `Debug`. Other types (tuples, user types): listed "not readable".
- `rocksdb_store.rs` (feature `rocksdb`, `rocksdb` 0.25, C++): a *secondary* instance (`open_cf_as_secondary`, its metadata in `$TMPDIR/moonkale-rocksdb-secondary-<hash>`), `try_catch_up_with_primary` before each read — a database another process writes stays openable and its new writes show up. Column families are the tables; `estimate-num-keys` the count; prefix scans seek. Brings `zstd-sys` (P-144).
- `is_redb_path` (`*.redb`), `is_rocksdb_path` (`*.rocksdb` directories — `.rdb` is a Redis dump) for every target; `open_redb`, `open_rocksdb`.
- Tests: `tests/stores.rs` (redb: typed tables, counts, scans, get, an unreadable table; RocksDB: families, prefix scan, a writer kept open and a write after opening seen). `examples/seed_stores.rs` seeds one store of each Milestone 17 kind for the E2E fixture.
- Redis/Dragonfly: designed only ([[Data Sources]], "Driver designs not built yet"); the stub modules were removed in Milestone 18 phase 1.
