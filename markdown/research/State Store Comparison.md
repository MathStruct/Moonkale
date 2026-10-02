---
title: "State store comparison — redb, SQLite, Turso, RocksDB, HelixDB"
description: The measurements behind ADR-0014 — five embedded engines behind the same StateStore interface, one workload modelled on Moonkale's own state, run on the dev machine and on GitHub's Linux, macOS and Windows runners, plus Android cross-builds; with the recommendation.
tags: [research, state, storage, benchmark]
---
Milestone 18 phase 5 ([[Milestone 18 - Library Refactor]]); decision record [[ADR-0014 One store for internal state]]; what the store has to hold: [[Internal State]]. Measured 2026-10-02.

## Method
- Every engine implements `moonkale_state::StateStore` (`packages/state`, one feature each) and passes the same **conformance suite** (missing reads, put/overwrite/delete, prefix-exact ordered scans, separate tables, ordered and atomic batches under a concurrent reader, versioned records, persistence across reopen).
- **Workload** (`packages/state/examples/compare.rs`), shaped like the state Moonkale writes: entity-log events of ~300 bytes keyed `folder · seq`, appended **one per commit** (an edit is one event) and in batches of 1 000 (an import); a replay (prefix scan of one folder); 200 settings writes of 2 KB (p50/p99); 10 000 random reads; reopening the full store; size on disk; a **kill -9** of a child process that writes two-key batches, then reopen and check that no batch is half there; whether a **second process** can open the store while it is open.
- **Durability** compared like with like: *durable* = an fsync per commit (survives a power loss); *relaxed* = survives a killed process but may lose the last moments of a power loss. redb has no relaxed mode that survives a killed process (`Durability::None` commits are lost unless a durable one follows), so it runs durable only.
- **Where**: the dev machine (Arch, NVMe, btrfs, 8 cores — with KDE's file indexer and Firefox running, so ±20 %), and GitHub's runners through `.github/workflows/state-compare.yml` (each backend built **alone and clean** in release, then the workload; Android: cross-build for `aarch64-linux-android`, API 24 = the app's `min_sdk`). Runner disks make fsync cheap, so compare engines within a row, not machines.

## Results on the dev machine

**Relaxed** (100 000 events), the mode an app would run in:

| backend | append, 1 per commit | append, 1 000 per commit | replay 100k | settings p50 / p99 | 10k gets | reopen | size | kill -9 | 2nd process |
|---|---|---|---|---|---|---|---|---|---|
| SQLite (WAL, `synchronous=NORMAL`) | **20 562/s** | 466 ms | 21 ms | 0.0 / 0.1 ms | 51 ms | 0.3 ms | 54.8 MB | ok | **yes** |
| Turso (`synchronous=NORMAL`) | 14 443/s | 1 487 ms | 88 ms | 0.0 / 0.1 ms | 142 ms | 0.7 ms | 58.1 MB | ok | no (locked) |
| RocksDB (WAL not synced) | **246 161/s** | 59 ms | 15 ms | 0.0 / 0.0 ms | 10 ms | 164 ms | **7.5 MB** | ok | no (locked) |

**Durable** (20 000 events) — everyone is fsync-bound here:

| backend | append, 1 per commit | append, 1 000 per commit | replay 20k | settings p50 / p99 | 10k gets | reopen | size | kill -9 | 2nd process |
|---|---|---|---|---|---|---|---|---|---|
| redb | 260/s | 118 ms | 1.9 ms | 3.8 / 7.9 ms | 7 ms | 3.3 ms | 11.3 MB | ok | no (locked) |
| SQLite (`synchronous=FULL`) | 247/s | 167 ms | 6.4 ms | 0.0 / 3.8 ms | 43 ms | 0.3 ms | 11.0 MB | ok | yes |
| Turso | 231/s | 350 ms | 21 ms | 0.0 / 3.6 ms | 107 ms | 0.9 ms | 59.1 MB | ok | no (locked) |
| RocksDB (WAL synced) | 244/s | 98 ms | 3.5 ms | 3.5 / 37.5 ms | 7 ms | 55 ms | 1.6 MB | ok | no (locked) |

**HelixDB** (embedded, used as key/value: a node label per table, the key as a hex property with an equality index, the value base64) — passes the conformance suite and the crash test, can be opened by a second process, but its cost grows faster than linearly: 1 000 events → 216 appends/s, a 1 000-event batch 5.2 s; 3 000 events → 121/s, the batch 46 s; **10 000 events did not finish within 25 minutes**. Without the index it was worse. The query language has no key range, so scans filter in Rust.

## Results on GitHub's runners (durable, 20 000 events)

| backend | clean release build: Linux / macOS / Windows | append 1 per commit: Linux / macOS / Windows | kill -9 everywhere | Android aarch64 (API 24) |
|---|---|---|---|---|
| redb | **18 / 35 / 46 s** | 1 214 / 1 168 / 1 057 per s | ok | builds (25 s) |
| SQLite | 68 / 49 / 56 s | 6 360 / 7 160 / 9 743 per s | ok | builds (50 s) |
| Turso | 212 / 262 / 412 s | 2 523 / 5 178 / 2 162 per s | ok | *failed at API 21 (`pwritev`); rerun at API 24 pending* |
| RocksDB | 406 / 340 / **860 s** | 5 929 / 11 204 / 4 358 per s | ok | *failed at API 21 (`io_posix.cc`); rerun at API 24 pending* |
| HelixDB | 503 / 408 / 741 s | (workload too slow at 20k) | — | builds (508 s) |

## Cost in the binary
Stripped size of the harness built with one backend (the harness alone: 0.4 MB): **redb 1.5 MB · SQLite 2.7 MB** · RocksDB 11.6 MB · Turso 22.2 MB · HelixDB 63.9 MB. In the app the SQLite and redb cost is already paid: `rusqlite` (the SQLite source) and `redb` (the redb source) are in the dependency tree today.

## What else counts
- **Two processes**: only SQLite (and Helix) let a second process open the store. Moonkale's windows share one process, but a CLI (`moonkale --project …`), an MCP helper or a crash-reporter would be a second one; with redb, Turso or RocksDB they would have to go through the running app.
- **Inspectable**: a SQLite file opens in any `sqlite3`, DB Browser, or Moonkale's own SQLite source — useful for debugging, export and support. redb and RocksDB need their own tools; Helix's object-store layout needs Helix.
- **Maturity**: SQLite and RocksDB are decades-old; redb is stable and pure Rust (1.x+); Turso is beta (`WITHOUT ROWID` still behind an experimental flag, found here); embedded HelixDB is an unpublished git dependency that writes a manifest even when opened read-only (P-145).
- **Async**: Turso and HelixDB have async APIs; behind a synchronous interface they need their own runtime (or `spawn_blocking`), the others are synchronous.
- **Pure Rust**: redb and Turso have no C/C++; SQLite (C, bundled) and RocksDB (C++, bindgen → libclang on every platform) do.

## Recommendation
**SQLite (`rusqlite`, bundled) as the store for Moonkale's state**, in WAL with `synchronous=NORMAL`:
1. crash-safe for a killed process, 20 000 small commits per second — two orders of magnitude more than Moonkale writes — and sub-millisecond settings writes;
2. the only fast candidate a second process can open;
3. already in the dependency tree, small (2.7 MB), builds in about a minute on every platform including Android;
4. a file everyone can open, and the most proven embedded store there is.

**redb is the runner-up**, and the reason the interface is shaped after it: pure Rust, the fastest to build and the smallest, but every commit is an fsync (~4 ms; fine off the UI thread), and one process only. If a pure-Rust core ever matters more than multi-process access, it drops in behind the same trait.

**RocksDB** wins raw throughput and size (compression) but costs 6–14 minutes of C++ per clean build, locks the store, and its strength (a write-heavy LSM) is not what Moonkale's state needs. **Turso** is promising but beta and the largest after Helix. **HelixDB** is not a key/value store and should not be used as one; it stays a candidate for the *second* store — a persisted graph + vector index — which is a separate comparison.
