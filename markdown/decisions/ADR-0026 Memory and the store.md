---
title: "ADR-0026 — What lives in memory, what goes to the store, and when"
tags: [adr, state, memory, performance, storage]
status: accepted
date: 2026-10-14
---
**Status:** accepted 2026-10-14 (Daniel), the day it was proposed, from his request ("a decision how much should be in memory and what and when things should be stored in a database"); written by Claude. Builds on [[ADR-0014 One store for internal state]] (SQLite), [[ADR-0025 Sessions are the outermost state]] and Daniel's standing preference: **Moonkale's state goes in a database, not in file dumps.** Measured by [[Benchmarks]].

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


## WP-56 implementation notes (2026-10-28)

Unsaved folder-file buffers use versioned `recovery` records in the existing
client state store (native SQLite; browser storage). Keys contain source id,
relative document path and the exact fetched base version. Records retain the
base text for conflict comparison. A serialized background task samples dirty
buffers every two seconds and on focus/visibility/document switches; unchanged
buffers cause no write. Native store operations run on worker threads. Save,
explicit reload and confirmed close/discard remove the document's versions.
The last interval can still be lost on a kill, and browser teardown cannot await
an asynchronous final write. Source writes retain optimistic version checks.

At startup, one collapsed notice offers Restore/Discard per file. A browser
Restore explicitly reopens its folder through the server jail; native sources
that are not open must be reopened before restoring. Restore fills memory only when both source version and base text match;
otherwise the comparison shows saved base, current text and recovered text.
It never writes the file automatically. An already dirty destination also
requires comparison. Hidden paths, credential/key filenames and oversized
buffers are excluded. Current source ignore rules and symlink components are
checked through the read-only `recovery-path` query; arbitrary secrets pasted into otherwise eligible text
cannot be classified automatically. Snapshots remain unencrypted at rest.

Initial safety limits (KiB; conservative defaults, **not BM-20–26 measurements**):

| Platform | Base + buffer | Recovery total | Content identities | Math markup | Image URLs | Sophia per folder |
|---|---:|---:|---:|---:|---:|---:|
| Desktop / server | 2048 | 16384 | 16384 | 8192 | 65536 | 262144 |
| Browser | 128 | 512 | 4096 | 2048 | 16384 | 65536 |
| Android | 512 | 4096 | 4096 | 2048 | 16384 | 65536 |

Recovery budgets charge encoded keys/values (JSON escaping included) and evict
least-recently-changed snapshots first. Content, math and image caches retain
their existing LRU eviction. Sophia is one consistent derived cache entry per
folder: exceeding its database/sidecar budget evicts that entry as a unit and
reports the ordinary file-index fallback. Authored stores are never evicted.
A web client's Sophia cache runs on its server and takes that host's budget.
Settings shows the client limits in one short paragraph. Browser storage's own
quota can refuse a write earlier, which is logged without buffer contents.

WP-21 already stores file-input hashes in Sophia manifests and skips unchanged
files on reopen; WP-66 already keys content identities by input/module hashes,
normalizer version and selector. WP-56 reuses both, without another index cache.
Math markup already reuses exact formula/macro inputs while mounted. Persistent
math/Typst output, aggregate cross-folder disk budgeting, Android memory-pressure
callbacks and BM-based tuning remain follow-ups; these initial limits do not
claim to bound the active graph or decoded DOM image working set.


Validation: snapshot bounds/keying/eviction, ignore/symlink admission, derived-only
folder eviction and persistent identity reuse pass in unit tests. The existing
six-file folder reopen regression reports zero parsed/written files and unchanged
manifests/log position. `recovery.mjs` passes reload → restore → save, stale-base
comparison and discard; a private desktop check killed the process with a dirty
buffer, restored it after restart and verified save removed the SQLite row.
All AGENTS gates and the complete CI browser matrix pass (2026-10-28).

### Folder-cache budget check (NEXT-33, 2026-10-28)

A private desktop opened `Moonkale-codex` itself and completed indexing
(1,398 walked files, 8,698 symbols; 43.2 seconds in the index build). After
shutdown/checkpoint, `.moonkale/code.sophia` occupied **38,367,232 bytes
(36.59 MiB)**; no nonempty WAL remained. The graph and status bar were inspected
in `target/next33-cache-repo.png`. Only this worktree was opened.

A disposable larger *definition-count* fixture, 1,000 Python modules with 20
functions each (20,000 definitions, 948,325 source bytes), occupied
**34,095,104 bytes (32.52 MiB)** plus a 32 KiB shared-memory sidecar and empty
WAL while open. This is more definitions, not more source bytes, than the real
repository; it is not a representative large monorepo benchmark. Evidence:
`target/next33-large-measure.log` and `target/next33-cache-large.png`. Both
private displays were stopped. Neither measurement justifies raising 256 MiB.

An eviction now latches the FolderStore handle into memory-only operation for
that folder open. Sync checks the latch before taking another code snapshot;
repair detection, rebuild and check cannot recreate the evicted database.
The existing status-bar fallback keeps the byte-budget reason. Reopening the
folder creates a new handle and permits another attempt. The regression evicts
with a one-byte limit, repeats sync/check/rebuild, checks that no database
reappears, and verifies authored stores and original files remain protected.
