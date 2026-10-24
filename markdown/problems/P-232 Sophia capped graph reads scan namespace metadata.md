---
title: "P-232 — Sophia capped graph reads scan namespace metadata"
tags: [problem, sophia, index, performance]
status: resolved
phase: "ADR-0029 folder store"
---
**Status:** resolved upstream (Sophia v0.1.2, pinned) · **Related:** [[Indexing]], [[ADR-0029 Sophia stores the code graph]]

## Problem

On the kept Moonkale folder replay store, `Source::query(All { limit: 1000 })`
takes seconds even though the answer contains only 1,000 namespaces and 999
Contains edges. Sophia v0.1.1 (`def22b0c`) spends nearly all of this time inside
`Interface::all`, before Moonkale converts its answer. Retained definition
history and text rendering are not the hot path in this answer.

`Interface::all` builds Contains edges from the retained keys, then calls
`edges_at` for every key. For a namespace, `edges_at(Out)` invokes
`children_in`, which constructs namespace/name views and Contains edges again.
The dependency filter in `all` discards all those Contains edges. Each namespace
listing nevertheless calls SQLite `meta_prefix`, whose `substr(key, …)` predicate
scans the metadata table. `namespace_exists` also uses `names_under(path, 2)`,
whose OR predicate scans the name index instead of seeking to the prefix.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Sophia skips namespace dependency expansion in `all`, and provides indexed metadata/name prefix reads | fixes the owning layer and benefits other clients | requires an upstream release |
| B | Moonkale bypasses the Interface or reimplements `all` | could avoid the work locally | duplicates Sophia semantics and query policy |

## Measurements

See [[Indexing]] for the debug/release phase table and the checked-in numeric
report. The copied Moonkale store has 11,541 definitions, 5,569 names and 15,951
metadata rows. The copied Lenticulum store has 2,775 definitions, 1,011 names
and 4,654 metadata rows. SQLite query plans report `SCAN t_meta` for
`meta_prefix` and `SCAN t_name` for `names_under`.

The reproducible `sources/sophia` example `query_profile` times `Interface::all`,
individual node reads, namespace children, and three ReadTx access paths. These
are separate probes, not nested exclusive spans; their times must not be added.
`MOONKALE_GRAPH_PROFILE=1` also splits the production Source path into `all`,
generic-call expansion, interface/open/scheduling, and adapter conversion.

## Decision

Request A upstream. Keep Sophia pinned and preserve query semantics. No Sophia
checkout edits or production workaround. Ask for deterministic access-count or
query-plan regression coverage upstream, rather than a wall-clock assertion.
The Moonkale changes only improve the numeric profiling example and spans.

## Lesson

A node cap does not bound work done before/after truncation. Avoid rebuilding
hierarchy edges during dependency discovery; prefix reads should seek even when
metadata stores contain large file manifests.

## Resolution (2026-10-24)

Sophia fixed all three points: `Interface::all` no longer expands namespace
children in its dependency loop (6105608); `meta_prefix` reads an indexed key
range and `names_under` seeks the root, the exact name and the descendants
separately (7680bed, with an EXPLAIN-QUERY-PLAN regression that fails on any
SCAN and a prefix conformance test on all four backends). Tagged v0.1.2
(`3c4b4a9`). On Moonkale's replayed store, All(1000) is 41 ms debug / 16 ms
release (was 6221 / 1846 ms); see [[Indexing]]. The eager record-id listing
before the cap was left as is; it isn't dominant.
