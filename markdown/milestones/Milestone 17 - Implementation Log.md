---
title: "Milestone 17 — Implementation Log"
description: What was built for "Embedded stores" — Turso, redb, RocksDB and embedded HelixDB as read-only sources, and LadybugDB moved to its shared library because its static one cannot be linked next to them.
tags: [milestone, log, sources, databases]
---
Plan: [[Milestone 17 - Embedded Stores]].

> [!success] Done 2026-09-28: 9 new Rust tests, 131 workspace tests pass, E2E `stores` plus `ladybug`, `links-sqlite` and `duckdb` pass
> **Four embedded stores open from the Sources tree:** a `*.turso` file (Turso, SQLite's dialect), a `*.redb` file, a `*.rocksdb` directory and a `*.helix` directory (embedded HelixDB). Each lists its tables, or its labels for HelixDB, and answers in the table editor in its own language: `sql`, `kv` (`scan`/`get`) or `helix` (`nodes`/`edges`). HelixDB's vertices and edges form a real graph. Everything is read-only for now; writes are the next step. **LadybugDB** had to move: its static library cannot share a binary with the new stores (P-144), so it now links as a *shared* library. The Arch package and development builds include it; the other packages ship without it for now. A draft issue for LadybugDB is ready.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | Compile spike of the four crates | ✅ each compiles alone (RocksDB's C++ took 113 s the first time) | HelixDB's engine is still only in git ([[002]]): pinned to `f0d1b554f4` |
| 2 | `core`: `TextDialect::{Kv, Helix}` and `TextDialect::name()`, used by the table editor, the agent's and MCP's `list_sources` | ✅ | three duplicated `match`es became one |
| 3 | `sources-kv`: `KvStore`/`KvSource`, the `kv` dialect, `RedbStore` (probes the stored types, dispatches to built-in types), `RocksStore` (a secondary instance, catches up before each read) | ✅ `tests/stores.rs` (2) + 3 unit tests | [sources-kv.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-kv/sources-kv.md) |
| 4 | `sources-sql`: `TursoSource` (mirrors SQLite, async) | ✅ `tests/turso.rs` | |
| 5 | `sources-graph`: `HelixSource` (labels, graph, `helix` dialect); `recursion_limit = 256` | ✅ `tests/helix.rs` (2) + parser test; result shape learned from a spike against a Disk store | |
| 6 | Wiring: desktop `open_database` and the server's `open_any` (both async now), Sources tree name checks, ids for attaching from another window | ✅ `stores.mjs` | |
| 7 | **The link failure** (P-144): LadybugDB and the new stores in one binary → 184 duplicate symbols. Traced to `liblbug.a` (whole-archive; bundled zstd 1.5.7 and SimSIMD). Reproduced with `lbug` + `zstd` alone, and with `lbug` + `simsimd` alone | worked around (Daniel's choice): LadybugDB as the `ladybug` feature, off by default; with it, the shared liblbug (`packaging/lbug-shared.sh`, rpath from desktop/web `build.rs`) | the shared library hides zstd; verified by linking all six drivers and by `ladybug.mjs` against a server with all of them |
| 8 | Packaging: the PKGBUILD downloads the pinned shared liblbug (x86_64/aarch64) and installs it as `/usr/lib/Moonkale/liblbug.so.0`, builds with `--features ladybug`; the flake gets `allowBuiltinFetchGit` (the Helix git dependency) and `bindgenHook` (RocksDB); the example `seed_people` requires `ladybug` | ✅ `makepkg --printsrcinfo`; **not yet built in CI** | Windows/macOS now compile RocksDB's C++ (bindgen needs libclang) and HelixDB's tree |
| 9 | E2E: `seed_stores` example → fixture `stores/`; `serve.sh` builds with the shared LadybugDB; `stores.mjs` | ✅ stores, ladybug, links-sqlite, duckdb | |

## Deviations from the plan
1. **LadybugDB is no longer in every build.** It is in the Arch package and in development/E2E builds through the shared library; other platforms ship without it until LadybugDB fixes its static library or we package the shared one everywhere. Daniel prefers the shared library everywhere and will raise it with LadybugDB (draft in the session's notes: the problem, a minimal reproduction, three possible fixes).
2. **RocksDB is a secondary instance, not "read-only":** it can open a database another process is writing, and it sees new writes.
3. **The clean-build cost was not measured.** The debug probe binary with all six drivers is 1.0 GB. The release packages from the next tag will show the difference.
4. **Graph Data mode for the key/value stores** shows database → tables only (a key/value table has no edges).

## Problems hit (→ [[Problem Log]])
- **P-144** LadybugDB's static library and zstd/SimSIMD from the new stores: duplicate symbols.
- **P-145** A HelixDB read-only open writes a manifest into the store.
- Writing `stores.mjs`: each table opens its own editor tab, so the helpers look only at the visible one; a regex selector matched the `events.rocksdb` file row before the `events · 3` table.
