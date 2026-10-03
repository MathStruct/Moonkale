---
title: "ADR-0014 — One store for Moonkale's internal state"
tags: [adr, state, storage]
status: accepted
date: 2026-10-01
---
**Status:** accepted 2026-10-02 — **SQLite** (`rusqlite`, bundled; WAL, `synchronous=NORMAL`), Daniel: *"Do SQL for now."* The direction is his (2026-09-28); the choice follows [[State Store Comparison]]. **Order (Daniel, 2026-10-01): the interface first, stable for redb as the reference implementation, then the comparison behind that interface** ([[Milestone 18 - Library Refactor]], phases 3 and 5).

**Progress:** the interface exists since 2026-10-02 (`moonkale-state`, [[Milestone 18 - Implementation Log]] phase 3a) with redb as the reference backend. **The comparison is done** ([[State Store Comparison]]): all five candidates pass the conformance suite and survive kill -9. **Recommended: SQLite** (`rusqlite`, WAL, `synchronous=NORMAL`) — fast enough by two orders of magnitude, the only fast one a second process can open, already in the tree, 2.7 MB, about a minute to build everywhere; redb runner-up. Accepted with SQLite on 2026-10-02.

## Context
Moonkale writes its own state to about a dozen places: JSON and JSON-lines files under each folder's `.moonkale/`, a user `settings.json` (desktop), `localStorage` (web), a third file on Android, and keeps the index only in memory. Nothing syncs between devices; app state is committed into repositories; the entity log's file format has known performance and correctness limits (P-086, issue #17). The full inventory is in [[Internal State]].

## Decision (proposed)
1. All internal state except secrets goes into **one embedded store per user** (two at most: state, and a persisted index), with the folder's id as part of the key — not into files inside the folder.
2. A folder keeps only what it deliberately shares with whoever clones it (KaTeX macros, its own wasm modules, shared annotations), as plain files.
3. The store syncs through the server hub once it exists; the index is never synced.
4. **Opening databases as sources stays an extension concern** and is not affected by the choice: the store is an implementation detail of the host, not a `Source`.
5. Candidates: Turso, redb, RocksDB, embedded HelixDB, with SQLite as the reference. The comparison measures write latency, cold open, build cost and binary size on every platform (Android included), crash safety and concurrency.

## Changing the engine later
Cheap by construction: every backend implements the same `StateStore`, every record is a versioned envelope, and `moonkale_state::copy(from, to, tables::ALL)` moves a whole store (tested SQLite → redb → memory, `packages/state/tests/copy.rs`). A switch is: open the new backend, copy once, use it. redb is the prepared alternative.

## Consequences
- `ext-api::settings`, `core::graph::history`'s persistence and the per-platform settings code collapse into one `state` service behind the `Workspace` ([[Milestone 18 - Library Refactor]], phase 5).
- Migration: read the old files once, write the store, keep the files for one release.
- A dependency that every platform must build becomes part of the core; that is why the build cost is a criterion, not an afterthought.
