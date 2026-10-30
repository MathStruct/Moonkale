---
title: "core — implementation notes"
tags: [crate-notes, milestone-1]
---
Implementation notes for `moonkale-core` (Milestone 1). Design: [[Graph-Native Model]]. Log: [[Milestone 1 - Implementation Log]].

## What is implemented
- `id.rs` — `SourceId(String)`, `NodeId(Uuid)`. `NodeId::derive(source, native_key)` is UUID v5 over `source ‖ 0x00 ‖ key` under a fixed namespace: deterministic, no lookup table, and the NUL separator prevents `("ab","c")` colliding with `("a","bc")` (tested).
- `graph/node.rs` — `Node { id, source, kind, label, native_key, content, version }`, `NodeKind` (open enum, `Custom(String)`), `ContentRef::{Text, Blob}`, `Version(u64)`. `language_hint()` is a temporary extension→language map until the index exists.
- `graph/edge.rs` — `Edge`, `EdgeKind`; only `Contains` is produced today.
- `source/query.rs` — `Query::{Node, Children}` → `QueryResult { nodes, edges }`.
- `source/transaction.rs` — `Transaction`/`Op::WriteText`/`TextPatch::Replace(Vec<Splice>)` with **char** offsets (tested on non-ASCII), `Applied` with per-op `OpResult`.
- `source/mod.rs` — the `Source` trait, async via `async-trait`.
- `error.rs` — `SourceError` (serializable so it crosses the server boundary intact).

## Critical decisions
- **No I/O, no runtime, no Dioxus.** Dependencies: `serde`, `uuid` (v5 only — no RNG, works on wasm with the `js` feature), `async-trait`, `thiserror`. This is what a WASM extension will see.
- **`?Send` on wasm32.** `#[cfg_attr(not(wasm32), async_trait)] #[cfg_attr(wasm32, async_trait(?Send))]` on the trait and on every impl. The `MaybeSendSync` supertrait does the same for the trait object. `async_trait` is re-exported so implementors don't take the dependency.
- **`Version` is opaque.** Consumers compare for equality only; sources choose the scheme.
- **Errors inside `Applied`, not `Err`.** A refused op is data; the transaction call itself only fails on transport/protocol errors.

## Not here (since Milestone 18 phase 1)
`graph/view.rs` (`GraphView`) and `command/` (descriptor, `when` clauses) were comment-only files and are removed; their designs are in the vault ([[Graph-Native Model]], [[Contribution Points]]). Commands live in `moonkale-ext-api`. `graph/property.rs` (`Value`) and `source/event.rs` (`Changes`) are real code now. `VERSION` (in `lib.rs`) is the release name from `MOONKALE_RELEASE` at build time, else the crate version.

## Tests
`cargo test -p moonkale-core` — 7 tests (id determinism/scoping/separator/display, patch application, char offsets, range errors).

## Milestone 4
`Op::CreateText { parent, name, text }` creates a text node under a directory (relative names allowed; missing directories created; refuses to overwrite). `Transaction::create_text`. `OpResult` is re-exported.

## Milestone 8: the entity log
`graph/history.rs` — `EventId` (ms timestamp in the high 64 bits, random low bits: sorts by time across replicas), `Event { id, at, actor, key, cause, kind }`, `EventKind::{Add{node, text}, Remove{node}, Rename{from, to, from_key, to_key}, Content{node, patch, chars_after, base}, Checkpoint{commit, message}}`. `EntityLog`: `append` keeps id order, `merge` is a set union, `for_node` follows rename chains backwards, `fold(until)` → `State { live, tombstones, renamed }`, `text_at(node, until)` replays from the last base (an `Add` with text, or a `Content` with `base`) through the patches, `to_jsonl`/`from_jsonl` (tolerant). Tests cover fold/replay/rename/round trip, bases for pre-existing files, ordering and merge. `serde_json` is now a normal dependency of core.

## Milestone 9
`EventKind::Snapshot { live, texts, folded }` and `EntityLog::compact(keep, at, actor)`: everything older than the last `keep` events folds into one snapshot placed at the boundary id (+1); checkpoints in the folded range are kept; `text_at` starts from the latest snapshot at or before `until` and replays the patches after it. Test `compaction_keeps_answers_and_checkpoints`.

## Milestone 18 phase 6.2 ([[ADR-0015 The core model]])
- `graph::Properties` (`BTreeMap<String, Value>`) as `Node::props` and `Edge::props`, empty and unserialized by default; `Value` compares floats by bits and is `Eq`.
- `NodeId::from_content(digest)` — content-addressed ids in their own UUID v5 namespace.
- `GraphView` and `subscribe` dropped from the design; `changes_since` is the change protocol.

## WP-06 hierarchy queries (2026-10-23)

`Query::Coarse { level, region }` returns a hierarchy cut using existing node
identities. Level zero keeps roots (or the region root); level one replaces
roots with direct children, retaining shallow leaves. `coarse.members` counts
hidden descendants, excluding the representative. `coarse.weight` counts
original edges with the same owner, projected endpoints and kind. Internal edges are
suppressed. Ancestors above the cut are navigation structure, not members.

`source::coarse::project` is the pure shared implementation; `coarse::query`
provides the snapshot fallback used by FolderSource, IndexSource and Unison.
Sophia supplies a metadata-only snapshot to the shared projection (P-229).
A source may override the query with indexed/cached cuts; community detection
can use the same query contract. Other sources explicitly return Unsupported.
The fallback refuses truncated snapshots, duplicate ids, multiple hierarchy
parents of equal precedence, cycles, absent hierarchy and cuts exceeding
100,000 nodes. Work is
bounded to 2,000,000 input nodes, 8,000,000 edges and requested levels 0–64.
It does not cache or accelerate source enumeration; source-native cuts remain
the optimization seam. No node bodies are fetched. Region queries return the
subtree cut and its internal relations, not external boundary proxies.

Core regression tests cover counts, weights, region membership, invalid forests,
and the output cap. The explicit `million_node_hierarchy` scale test generates
1,000 folders × 999 files plus one root (1,000,001 nodes), reduces it to 1,000
nodes and 999 cross-folder relations in 1.32 seconds in the development native
test build. This is projection time, not source I/O or browser first paint.

The response table has one row with `source.nodes` and `source.edges`; the
selector uses the complete input count without requesting a million-node wire
snapshot. `Contains` takes precedence over file-level `Defines` for nested
scopes. For sources without All, the fallback walks Children metadata from the
root, queuing directories only. Sophia assigns a shared content definition to
its smallest stable name id for navigation and retains every original Names
relation; aliases do not multiply membership counts.


### Proposals and review (2026-10-30)

`Source::preview_text` checks bounded text proposals without writing. `Capabilities::atomic_text` explicitly promises text-only transactions are all-or-nothing; ordinary sources continue reporting ordered per-op results. Read versions still come from `fetch_text`, not query metadata.

### Typed row writes (WP-14, 2026-10-30)

`source::rows` defines `RowPage`/`RowSnapshot`, source-checked `RowKey`, and
bounded `RowBatch` operations (`Update`, `Insert`, `Delete`). Every update/delete
carries the fetched row version; inserts assert keyed absence with `None`.
`Source::read_rows`, `preview_rows` and `apply_rows` default to Unsupported.
`Capabilities::atomic_rows` promises the whole batch commits or rolls back.
This is separate from text patches: no SQL text, DDL or arbitrary commands are
represented by a row operation. The authenticated remote adapter forwards typed
requests to the authoritative source host for validation and execution.


### Typed table queries (WP-13, 2026-10-30)

`Source::query_rows` defaults to Unsupported. `TableCapabilities` advertises sort,
filter and keyset independently, false for old descriptors. `TableQuery` carries
up to eight sort keys, 32 ANDed predicates, 1–1000 rows and a native-valued
`TableCursor` (sort values plus complete row key); requests cap at 64 KiB.
`RowPage::next` is optional and defaults to None on old wire snapshots. The SQL
implementation preserves row snapshots and their expected versions.
