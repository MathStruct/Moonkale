---
title: "ADR-0014 — One store for Moonkale's internal state"
tags: [adr, state, storage]
status: proposed
date: 2026-10-01
---
**Status:** proposed — the direction is Daniel's (2026-09-28); *which* store waits for the comparison described in [[Internal State]]. **Order (Daniel, 2026-10-01): the interface first, stable for redb as the reference implementation, then the comparison behind that interface** ([[Milestone 18 - Library Refactor]], phases 3 and 5).

**Progress:** the interface exists since 2026-10-02 (`moonkale-state`, [[Milestone 18 - Implementation Log]] phase 3a) with redb as the reference backend; the comparison is next.

## Context
Moonkale writes its own state to about a dozen places: JSON and JSON-lines files under each folder's `.moonkale/`, a user `settings.json` (desktop), `localStorage` (web), a third file on Android, and keeps the index only in memory. Nothing syncs between devices; app state is committed into repositories; the entity log's file format has known performance and correctness limits (P-086, issue #17). The full inventory is in [[Internal State]].

## Decision (proposed)
1. All internal state except secrets goes into **one embedded store per user** (two at most: state, and a persisted index), with the folder's id as part of the key — not into files inside the folder.
2. A folder keeps only what it deliberately shares with whoever clones it (KaTeX macros, its own wasm modules, shared annotations), as plain files.
3. The store syncs through the server hub once it exists; the index is never synced.
4. **Opening databases as sources stays an extension concern** and is not affected by the choice: the store is an implementation detail of the host, not a `Source`.
5. Candidates: Turso, redb, RocksDB, embedded HelixDB, with SQLite as the reference. The comparison measures write latency, cold open, build cost and binary size on every platform (Android included), crash safety and concurrency.

## Consequences
- `ext-api::settings`, `core::graph::history`'s persistence and the per-platform settings code collapse into one `state` service behind the `Workspace` ([[Milestone 18 - Library Refactor]], phase 5).
- Migration: read the old files once, write the store, keep the files for one release.
- A dependency that every platform must build becomes part of the core; that is why the build cost is a criterion, not an afterthought.
