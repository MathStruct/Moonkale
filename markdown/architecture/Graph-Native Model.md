---
title: "Graph-Native Model"
tags: [architecture, core]
---
Crate: `packages/core`. The model is the contract between sources, editors, the index, extensions and agents.

> [!note] As built (Milestone 18 phase 6.2, 2026-10-03)
> The design and the code agree since [[ADR-0015 The core model]]: nodes and edges carry `props` (`Properties`, sorted typed values); `GraphView` and `subscribe` left the design (editors hold `QueryResult`s; sources report changes through the `changes_since` long poll); ids are derived from `(SourceId, native key)`, or content-addressed with `NodeId::from_content(digest)`. Not built: `fetch(NodeId) -> Content` (it is `fetch_text` / `fetch_bytes`), `ContentRef::{Rows, Nested}`, `Custom(ExtensionId, String)` kinds (they are `Custom(String)`). The entity log (`graph::history`) is in [[Version Management]].

## Entities

```mermaid
classDiagram
  class Node {
    NodeId id
    SourceId source
    NodeKind kind
    String label
    String native_key
    ContentRef? content
    Version version
    Properties props
  }
  class Edge {
    SourceId source
    NodeId from
    NodeId to
    EdgeKind kind
    Properties props
  }
  class Source {
    <<trait>>
    descriptor()
    query(Query) QueryResult
    fetch_text(NodeId) / fetch_bytes(NodeId)
    apply(Transaction) Applied
    changes_since(seq) Changes
  }
  Node "1" --> "*" Edge : from/to
  Source --> Node : owns
```

### Design choices

**Open enums for kinds.** `NodeKind` and `EdgeKind` have well-known variants editors can match on (`File`, `Row`, `Vertex`, `Page`, `Symbol`, `Block`; `Contains`, `References`, `Links`, `ForeignKey`) plus `Custom(ExtensionId, String)`. Extensions add kinds without touching `core`; editors advertise which kinds they open ([[Contribution Points]]).

**Content is a reference, not bytes.** `ContentRef::{Text, Blob, Rows, Nested}` describes shape and size; `Source::fetch` gets the body. This is what makes "open a 10 GB folder" or "open a billion-row table" an *open*, not a *load*. It also mirrors the old README's "discourage very large files".

**Deterministic ids for sourced entities.** A file's `NodeId` is derived from `(SourceId, relative path)`; a row's from `(SourceId, table, primary key)`. Re-opening yields the same ids with no lookup table, so saved layouts, links and agent citations survive restarts.

**Queries, not views.** Editors hold the `QueryResult` of a query and re-run it when the source reports a change (`changes_since`). The graph editor, the table editor and an LLM tool call all run queries — "display a subgraph" is just "run a narrower query". (A `GraphView` with a version watermark was the design until [[ADR-0015 The core model]] dropped it.)

**Identity: derived, or content-addressed.** By default an id is derived from `(SourceId, native key)`. A source whose entities *are* their structure (Sophia's declarations and proofs) uses `NodeId::from_content(digest)`: the same structure is the same node everywhere, and names become edges to it.

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
