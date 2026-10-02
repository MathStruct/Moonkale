---
title: "state-stores — implementation notes"
tags: [crate-notes, milestone-18]
---
Notes for `moonkale-state-stores` (Milestone 18 phase 5). Measurements: [[State Store Comparison]]; decision: [[ADR-0014 One store for internal state]].

The engines behind `moonkale_state::StateStore`, one feature each; only the apps depend on this crate.

- `sqlite_store.rs` (`sqlite`) — **the app's**: `rusqlite` bundled, WAL; `open_with(Durability)` = `synchronous=FULL` or `NORMAL`; one table `kv(t, k, v)` keyed `(t, k)` `WITHOUT ROWID`; prefix scans as ranges (`sql.rs::prefix_end`). Connection behind a mutex.
- `redb_store.rs` (`redb`) — the reference the interface was shaped against; one redb table per state table; every commit durable (redb has no relaxed mode that survives a killed process).
- `turso_store.rs` (`turso`) — the same schema as SQLite but a rowid table (`WITHOUT ROWID` is experimental in Turso 0.7); async, bridged with its own Tokio runtime (do not call from inside an async task).
- `rocks_store.rs` (`rocksdb`) — one column family, the table as a key prefix, `WriteBatch`; `Relaxed` = WAL not synced.
- `helix_store.rs` (`helix`) — HelixDB embedded used as key/value: a node label per table, hex keys, base64 values, an equality index on the key, conflicts of its optimistic transactions retried; slow beyond a few thousand entries (see the comparison).
- `examples/compare.rs` — the comparison workload (`COMPARE_ONLY`, `COMPARE_MODE`); `tests/backends.rs` runs every enabled engine through the conformance suite; `tests/copy.rs` copies SQLite → redb → memory.
