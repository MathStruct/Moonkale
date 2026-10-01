---
title: "Graph-Native Model"
tags: [architecture, core]
---
Crate: `packages/core`. The model is the contract between sources, editors, the index, extensions and agents.

> [!note] As built vs. this design (checked 2026-10-01)
> The diagram below is the **design**. What `core` has today:
> - `Node { id, source, kind, label, native_key, content: Option<ContentRef>, version }` — **no property map**; `ContentRef` is `Text { len, lang }` or `Blob { len, mime }` (no `Rows`/`Nested`).
> - `Edge { source, from, to, kind }` — **no id, properties or weight**.
> - `NodeKind` / `EdgeKind` as listed below, with `Custom(String)` (not `Custom(ExtensionId, String)`).
> - `Source`: `id`, `descriptor`, `query`, `fetch_text`, `fetch_bytes`, `apply`, `refresh`, `changes_since` (a long poll, Milestone 16) — **no `fetch(NodeId) -> Content`, no `subscribe`**.
> - `Query`: `Node`, `Children`, `Neighbours { node, depth, direction }`, `All { limit, kinds }`, `Text { dialect, text }`; results are `QueryResult { nodes, edges, table, truncated }`. **`GraphView` is a design stub** (`core/src/graph/view.rs`); editors hold `QueryResult`s.
> - `Capabilities { read, write, watch, text_query }`; `TextDialect` = SQL, Cypher, TypeQL, `kv`, `helix`.
> - Ids are derived from `(SourceId, native key)` as designed; the entity log (`graph::history`) exists ([[Version Management]]).
>
> Whether properties, edge ids and `GraphView` are added or dropped from the design is phase 6 of [[Milestone 18 - Library Refactor]]; content-addressed ids are a candidate there too.

## Entities

```mermaid
classDiagram
  class Node {
    NodeId id
    SourceId source
    NodeKind kind
    String label
    PropertyMap props
    ContentRef? content
    Version version
  }
  class Edge {
    EdgeId id
    SourceId source
    NodeId from
    NodeId to
    EdgeKind kind
    PropertyMap props
    f32? weight
  }
  class Source {
    <<trait>>
    descriptor()
    query(Query) QueryResult
    fetch(NodeId) Content
    apply(Transaction) Applied
    subscribe() Stream~SourceEvent~
  }
  class GraphView {
    Query query
    nodes, edges
    Version watermark
  }
  Node "1" --> "*" Edge : from/to
  Source --> Node : owns
  GraphView --> Node : materialises
```

### Design choices

**Open enums for kinds.** `NodeKind` and `EdgeKind` have well-known variants editors can match on (`File`, `Row`, `Vertex`, `Page`, `Symbol`, `Block`; `Contains`, `References`, `Links`, `ForeignKey`) plus `Custom(ExtensionId, String)`. Extensions add kinds without touching `core`; editors advertise which kinds they open ([[Contribution Points]]).

**Content is a reference, not bytes.** `ContentRef::{Text, Blob, Rows, Nested}` describes shape and size; `Source::fetch` gets the body. This is what makes "open a 10 GB folder" or "open a billion-row table" an *open*, not a *load*. It also mirrors the old README's "discourage very large files".

**Deterministic ids for sourced entities.** A file's `NodeId` is derived from `(SourceId, relative path)`; a row's from `(SourceId, table, primary key)`. Re-opening yields the same ids with no lookup table, so saved layouts, links and agent citations survive restarts.

**Views, not vectors.** Editors hold a `GraphView` = query + materialised subgraph + version watermark. Refresh is "re-run the query"; incremental update is "apply events above the watermark". The graph editor, the table editor and an LLM tool call all return `GraphView`s — "display a subgraph" is just "run a narrower query".

**Two query levels.** A structured `Query` IR every source must support (listing, neighbourhoods, paging, search) so every editor works with every source; and `TextQuery { dialect, text }` for raw SQL/Cypher/TypeQL. Results from text queries are *lifted* into nodes/edges using rules the source provides ([[Data Sources]]).

**Writes are transactions of ops with expected versions.** Optimistic concurrency, undo/redo by replay, partial success with `Unsupported` reasons. Text edits are patches ([[ADR-0009 Patches not snapshots]]).

**Cross-source edges** are owned by the source that stores them; the target may be foreign. This enables "wiki page links to a database row" and is the trickiest consistency problem in the project — see [[Problem Ranking]] (R-16).

## What the model is *not*
- Not a database. Nothing is persisted by `core`; sources and the index persist.
- Not a CRDT. Versions + patches leave the door open ([[ADR-0009 Patches not snapshots]]).
- Not yet versioned. `Version` is a change marker; the append-only entity log that gives nodes and edges a history (added/removed by UUID, with timestamps) is designed in [[Version Management]] / [[ADR-0012 Two histories]].
- Not typed beyond kinds. Schema (TypeDB types, SQL columns) is exposed as a *schema graph* in `SourceDescriptor`, not enforced by `core`.

## Worked example: a Rust crate in a folder

| Thing | Node kind | Edge to parent |
|---|---|---|
| `src/` | `Directory` | `Contains` from crate root |
| `src/main.rs` | `File{Text, lang: rust}` | `Contains` |
| `fn main` | `Symbol` (from index) | `Defines` from file |
| `App` used in `main` | — | `References` from `main` symbol to `App` symbol |
| `docs/arch.md` with `[[main.rs]]` | `Page` | `Links` from page to file |
| a `users` table in Postgres | `Table` | `Contains` from schema |
| the page also says `[[users]]` | — | `Links` page → table (cross-source) |

All of that is one graph; the [[Graph View]] draws it, the [[Table Editor]] pages through `users`, and an agent can walk from the doc to the code to the data.
