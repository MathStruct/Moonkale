---
title: "Milestone 17 — Implementation Log"
description: What was built for "Embedded stores" — Turso, redb, RocksDB and embedded HelixDB as read-only sources, and LadybugDB kept next to them by making its bundled zstd/SimSIMD symbols local (lbug 0.21).
tags: [milestone, log, sources, databases]
---
Plan: [[Milestone 17 - Embedded Stores]].

> [!success] Done 2026-09-28: 9 new Rust tests, 131 workspace tests pass, E2E `stores` plus `ladybug`, `links-sqlite` and `duckdb` pass
> **Four embedded stores open from the Sources tree:** a `*.turso` file (Turso, SQLite's dialect), a `*.redb` file, a `*.rocksdb` directory and a `*.helix` directory (embedded HelixDB). Each lists its tables, or its labels for HelixDB, and answers in the table editor in its own language: `sql`, `kv` (`scan`/`get`) or `helix` (`nodes`/`edges`). HelixDB's vertices and edges form a real graph. Everything is read-only for now; writes are the next step. **LadybugDB**'s static library could not share a binary with the new stores (P-144). After Daniel's report upstream, lbug 0.21 can make its bundled symbols local; with that, LadybugDB links statically again in every Linux build (macOS/Windows: without it for now).

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
| 9 | E2E: `seed_stores` example → fixture `stores/`; `stores.mjs` (`serve.sh` wrapped dx with the shared LadybugDB until step 10) | ✅ stores, ladybug, links-sqlite, duckdb | |
| 10 | **Switch-over (2026-09-29)** after lbug 0.21.0 (the upstream answer to Daniel's issue): `lbug = "0.21"`, `LBUG_LOCALIZE_BUNDLED_SYMBOLS=1` in a new `.cargo/config.toml`; the `ladybug` feature of desktop/web/api, `lbug-shared.sh` and both `build.rs` removed; desktop/api pull `sources-graph/ladybug` under `cfg(target_os = "linux")` (`open_ladybug` returns `None` without the feature); PKGBUILD without the shared library; the flake's `liblbug` derivation (0.21.0) localizes the archive itself, since the crate skips that for `LBUG_LIBRARY_DIR` | ✅ desktop links all six drivers statically (no `liblbug.so`); the localized archive has 0 global zstd/simsimd symbols and keeps 180 `lbug_*`; a probe opened LadybugDB, Turso, HelixDB and RocksDB in one binary, with the crate's download and with a Nix-style external archive; a 0.20.4 `people.lbug` opens; 132 workspace tests and E2E `stores`, `ladybug`, `links-sqlite`, `duckdb` pass (`stores` timed out once as the first suite on a cold server, passed on rerun) | Nix itself not run (no nix here) |

## Deviations from the plan
1. **LadybugDB is Linux-only for now.** Localizing needs ELF tools, so macOS and Windows builds leave it out. (Steps 7–9 first used the shared library, Arch only; step 10 replaced that.)
2. **RocksDB is a secondary instance, not "read-only":** it can open a database another process is writing, and it sees new writes.
3. **The clean-build cost was not measured.** The debug probe binary with all six drivers is 1.0 GB. The release packages from the next tag will show the difference.
4. **Graph Data mode for the key/value stores** shows database → tables only (a key/value table has no edges).

## Problems hit (→ [[Problem Log]])
- **P-144** LadybugDB's static library and zstd/SimSIMD from the new stores: duplicate symbols.
- **P-145** A HelixDB read-only open writes a manifest into the store.
- Writing `stores.mjs`: each table opens its own editor tab, so the helpers look only at the visible one; a regex selector matched the `events.rocksdb` file row before the `events · 3` table.
