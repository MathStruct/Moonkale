---
title: "ADR-0026 — What lives in memory, what goes to the store, and when"
tags: [adr, state, memory, performance, storage]
status: proposed
date: 2026-10-14
---
**Status:** proposed 2026-10-14, from Daniel's request ("a decision how much should be in memory and what and when things should be stored in a database"); written by Claude. Builds on [[ADR-0014 One store for internal state]] (SQLite), [[ADR-0025 Sessions are the outermost state]] and Daniel's standing preference: **Moonkale's state goes in a database, not in file dumps.** Measured by [[Benchmarks]].

## Context

Moonkale holds four kinds of data, and they have different owners:
- **What the user authored about their work:** sessions, settings, layouts, conversations, comments, run records, the entity log. **Moonkale owns it.**
- **What a source holds:** files, database rows, graph nodes. **The source owns it.**
- **What tools derive:** the index, search and embeddings, layouts, rendered math, IR, inferences ([[Shared Work]] §5). **No one authored it; it can be recomputed, at a cost.**
- **What is in use right now:** open documents, unsaved edits, the graph scene, a query's result, a live run's stream. **It's the working set.**

Today the index is rebuilt on every open and kept only in memory; the state store already holds settings, layouts, the entity log and agent conversations.

## Decision

| Data | Truth | In memory | In the store, and when |
|---|---|---|---|
| Sessions, settings, layouts, conversations, comments, run records' metadata | the **state store** | a cache of what is open | **write-through**, debounced to at most 1 s after the change; on switch, close and quit at once |
| The entity log | the state store | the tail being written | **appended per event,** in batches of at most 250 ms |
| **Unsaved edits** | memory, until saved to the source | the buffer | a **recovery snapshot** every 2 s while dirty and on blur, switch or quit. On restart, an unsaved buffer comes back (crash safety). The snapshot is deleted on save |
| A source's content (files, rows, nodes) | **the source** | open documents; bounded caches of what was read | **never copied wholesale** into the store; only what the user saved as theirs (a saved query, a pinned answer) |
| Derived data, **expensive** (the index, embeddings, layouts of large graphs, rendered math SVG, Typst output, inferences) | recomputable | while in use | **persisted, keyed by input hashes** (content-addressed), so reopening reuses it, and a changed input invalidates exactly its own entries (WP-21) |
| Derived data, **cheap** (syntax highlighting, small layouts, a table's sort order) | recomputable | while in use | not stored |
| Query results | the source | up to the source's cap (50 000 rows), with pushdown paging past it (WP-13) | only when saved (a saved answer, a plot's data) |
| The graph scene | the engine | what is drawn, plus the coarse tier's aggregates | node positions, as part of the layout (above) |
| Live run streams (beliefs, losses, ticks) | the run | a **ring buffer** of the recent window | the run record keeps a downsampled series; a full trace goes to **DuckDB/Parquet** beside the run, because its volume is analytical, not state |
| External data caches (market data, remote files) | the provider | what is in use | on disk in the right engine (DBN and Parquet for market data, read through DuckDB), with a **byte budget** and LRU eviction |
| Secrets | the system keychain or the environment | resolved when needed, dropped after use | **never** |

**Rules that apply to all of it:**
1. **Nothing is written on the input path.** A keystroke's frame never waits for a store write (CED-R22). Writes are debounced and run off the UI thread.
2. **Every cache has a byte budget per platform** (desktop, server per session, web, the phone) and evicts least-recently-used first. On the phone, memory pressure (Android's trim callbacks) drops caches before Android kills the app.
3. **The engine fits the data:**
   - SQLite (the state store) for state;
   - DuckDB and Parquet for bulk analytical data (traces, market data, benchmark history);
   - memory for the working set;
   - plain files only for what users share through git (exports such as comments' JSONL).
4. **Persisted derived data is rebuildable:** deleting the derived tables loses no work, only time. This is also the answer to any corruption of them.

## Consequences

- **New tables:** recovery snapshots of unsaved buffers, derived-data entries keyed by input hash (the index, embeddings, rendered math), cache indexes with sizes. Run traces go to their own DuckDB file per run or session.
- **The index persists** (WP-21) instead of rebuilding on every open, so BM-03's warm time drops to loading.
- **The budgets are numbers to set** from [[Benchmarks]] BM-20 to BM-26, per platform, recorded in this ADR when measured.
- **Not decided:**
  - whether derived data is shared across sessions (proposed: yes, keyed by input hash, so two sessions on one repository share one index);
  - encryption at rest for snapshots of unsaved buffers ([[Security]], data at rest).

Work package: WP-56 in [[Work Packages]].
