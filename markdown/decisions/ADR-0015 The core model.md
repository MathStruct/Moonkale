---
title: "ADR-0015 — The core model: properties, no GraphView, content ids, changes_since"
tags: [adr, core, library]
status: accepted
---
**Accepted 2026-10-03** (Milestone 18 phase 6.2; decisions left to "free hand" by Daniel, 2026-10-01). Context: [[Graph-Native Model]], [[Milestone 18 - Library Refactor]], [[Lenticulum goal|Lenticulum]] and Sophia as the outside users the library is for.

## Context
`moonkale-core` becomes something other repositories depend on (by git tag `lib-vN`, phase 6.3). Its design note described four things the code never had — node/edge properties, `GraphView`, `Source::subscribe`, and (as a candidate) content-addressed ids. Before the first tag each one is either built or removed from the design, because adding a field to `Node` later breaks every source.

## Decisions
1. **Properties on nodes and edges — built.** `Node::props` and `Edge::props`: `Properties = BTreeMap<String, Value>`, empty by default and not serialized when empty, so stored and transmitted shapes are unchanged for every source that has none. Sorted for stable comparison and serialization. `Value` compares floats by their bits, so `Node` stays `Eq`. Users: a factor's parameters (Lenticulum), a declaration's hash and kind (Sophia), later a row's columns. Edge ids and `weight` are **not** added: an edge is identified by `(source, from, to, kind)`, and a weight is a property.
2. **`GraphView` — dropped.** Editors hold `QueryResult`s and re-run the query when a source reports a change. "Apply events above a watermark" needs per-node change events no source produces; until one does, a view is a query.
3. **Content-addressed ids — an option, built.** `NodeId::from_content(digest)`: UUID v5 of a caller-computed digest of the *normalised* structure, in a namespace of its own (never equal to a `derive`d id). The same structure is the same node in every source and on every machine — Sophia's model, where many names point at one node (names are edges). Ids stay 128-bit; a source that must verify keeps the full digest as a property. `derive(source, key)` stays the default.
4. **`changes_since`, not `subscribe` — decided.** The long poll of Milestone 16 works over HTTP server functions and in wasm, needs no stream type in `core`, and survives reconnects (`seq`, `reset`). `subscribe() -> Stream` leaves the design.

## Consequences
- 38 `Node`/`Edge` literals across the sources gained `props: Default::default()` (one mechanical commit). Shapes on disk (the entity log) and on the wire are unchanged when `props` is empty.
- [[Graph-Native Model]] shows the model as built; the design diagram is updated to match.
- `lib-v1` (phase 6.3) is tagged with this model.
