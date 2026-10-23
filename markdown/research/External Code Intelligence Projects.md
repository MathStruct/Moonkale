---
title: "Recoco and Thread — requirements and fit for Moonkale"
description: What two neighbouring code-intelligence projects require and propose, how that fits Moonkale, and whether it would help, as requirements prior art only (Thread is AGPL-licensed, so no code is reused).
tags: [research, indexing, graph, requirements]
---
First draft by Codex (2026-10-11) from the local checkouts `../recoco-main/` and `../thread-main/`; trimmed by Claude to requirements and fit (2026-10-12). This is a **research inventory**, not a commitment to build anything. Both projects describe themselves as unmaintained or incomplete, so every status below is what their documents claim, not what was tested.

**2026-10-23:** [[ADR-0029 Sophia stores the code graph]] drops the plan for a persisted index of Moonkale's own. The persisted, incremental store is a Sophia store per folder. The requirements below stay as WP-21's checklist for it.

> [!warning] Licences: ideas and requirements only, no code
> **Thread is AGPL-3.0-or-later.** Moonkale is MIT. No Thread code, schema or text may be copied into Moonkale, and nothing here depends on doing so. The note records *requirements and design ideas*, which licences do not cover, so that Moonkale can write its own specs. **Recoco is Apache-2.0**, which is compatible with MIT, but nothing below proposes reusing its code either; if that ever changes, it needs its own decision and attribution.

## Verdict: would it help Moonkale?
**Yes, as requirements; no, as software.**
- **What Moonkale gains:** a checklist for the part it lacks most. The index is in memory and rebuilt on every open ([[Indexing]], as built). Thread and Recoco are both about **incremental, provenance-tracked derived data**:
  - re-analyse only what changed;
  - know where each fact came from and whether it is stale;
  - bound every query;
  - fail visibly, with partial results instead of empty ones.

  Those requirements transfer directly to the index, the graph view and the agent tools.
- **What Moonkale should not take:** either project's architecture or stack (Postgres/D1/Qdrant/Vectorize, a dataflow engine in the core, a multi-tenant streaming service, conflict prediction). Moonkale runs local-first on desktop, web and Android, with SQLite for its own state ([[ADR-0014 One store for internal state]]).
- **The precondition is already in place:** derived facts need ids that survive edits. Since 2026-10-11 symbol ids no longer change when lines move (path keys, see the index crate note), and [[ADR-0019 Extension coordinates and block addressing]] (proposed) defines how sub-file blocks are named.

## Recoco: what it is and requires
Recoco is an incremental data-processing (ETL) engine: sources → transformations → targets, with lineage and memoisation, so that only work downstream of a change re-runs. Its documented requirements:
- Compose sources, transforms and targets as a dataflow; track lineage; re-run only affected work.
- An async Rust API, with every connector and transform behind its own Cargo feature; no mandatory Python or heavy dependencies.
- Both **transient** (in-memory) and **persisted, incremental** execution. Transient use must not need a database.
- Backpressure: bounded rows and bytes in flight per source.
- Typed configuration and explicit context instead of global setup (the v1 proposal criticises its own `init_lib_context`).

Its v1 proposal (`docs/v1-way-ahead.md`, a draft) leaves the hard parts open, and these are exactly the questions an incremental index has to answer too:

| area | open requirement |
|---|---|
| Invalidation | How a change to a function's code invalidates cached results; explicit invalidation (`version = N`); never re-run unchanged inputs. |
| Row identity | Stable keys for outputs, so the engine can reconcile inserts, updates and deletes with what it produced before. |
| Change detection | Stable-key diffs for files, change data capture for databases, debounced file watching that propagates into dependent work. |
| Errors | Public error types; user versus backend errors; retry propagation. |
| Concurrency | Shared resources across parallel work without unsafe code or excessive cloning. |

## Thread: what it is and requires
Thread is a code-intelligence system. It parses repositories into an incremental, content-addressed graph, answers dependency and semantic queries, streams progressive results, and later predicts merge conflicts. Its main spec (`specs/001-realtime-code-graph/spec.md`, a draft) has four user stories:
1. **Graph queries:** callers, callees, data flow and semantic relations, each with confidence and provenance.
2. **Conflict prediction:** watch saves, find affected callers, report likely conflicts.
3. **Multi-source:** local, Git-hosted and S3 repositories in one graph, with attributed and incrementally updated cross-repository links.
4. **AI conflict assistance:** explain conflicts and propose a merge order; the developer decides.

The requirements worth knowing, condensed (FR numbers are Thread's):

| topic | requirement |
|---|---|
| Extraction (FR-001–002, FR-010) | Parse supported languages and derive calls, types, data flow, imports and exports into a multi-level graph; detect language by extension, then by parse fingerprint. |
| Incremental work (FR-008, FR-012) | Re-analyse changed files and their affected dependents only; skip unchanged content by hash. |
| Identity (FR-004) | Graph data stored by content address: equal hash means equal data. |
| Provenance (FR-014) | Every fact keeps its source, version and timestamp. |
| Latency (FR-005–007, FR-013) | Dependency queries under 1 s at 100 000 files; progressive results (syntactic < 100 ms, structural < 1 s, semantic < 5 s); a change reaches clients in < 100 ms locally. Even on timeout, send the last known result, never silence. |
| Honest degradation (FR-015, FR-024) | If vector search is unavailable, say so (`search_mode`) instead of silently falling back to keywords; `allow_partial` returns the available subgraph plus what is missing. |
| Bounded traversal (FR-025, streaming contract) | Cycle detection and depth limits; lazy, paginated, iterative traversal; never load the whole graph on a constrained runtime. |
| Committed vs local (FR-017–018) | Query the committed base plus local edits by default, committed-only on request; stay consistent during edits. |
| Operations (FR-021, FR-026–027) | Health and metrics endpoints; structured logs with timing and context. |
| Classification (FR-CLASSIFY, FR-LANGQUERY) | Map syntax nodes to ~22 language-independent semantic classes, so callers can ask for "callable definitions" without knowing each grammar's node names. |

Its knowledge-layer design (`docs/architecture/AI_KNOWLEDGE_LAYER_DESIGN.md`, options analysis) adds requirements for **agent context**:
- deliver a manifest first and fetch details on demand, instead of one large context pack;
- a small tool set: status, locate, changed-since, pagination;
- a graph → parser → text-search fallback that labels each answer's source and confidence;
- tree-sitter `tags.scm` for definitions instead of a bespoke extractor per language;
- reconstruct syntax trees on demand rather than storing them (its storage estimate for keeping them grew from 1.5× to 5–15× the raw code).

Its own status documents disagree with each other about what is implemented: the README says production-ready, while compliance reports say incremental updates are missing. So none of its numbers are evidence; they are targets.

## Fit with Moonkale

| requirement | Moonkale today | fit |
|---|---|---|
| Graph of code and derived relations | `Node`/`Edge`/`Query`, `moonkale-index`, tree-sitter symbols, wiki-links, graph view | **Strong.** Calls, imports and data flow fit the model; precision and provenance must become explicit. |
| Stable identity for derived facts | Path keys for symbols (2026-10-11), `NodeId::from_content` ([[ADR-0015 The core model]]), block anchors (ADR-0019, proposed) | **Strong now.** It was the missing precondition until the symbol-id fix. |
| Incremental, persisted index | In memory, rebuilt on open; `changes_since` and file watching exist as inputs | **Gap, and the most valuable transfer.** |
| Provenance and staleness | Entity log for edits; nothing per derived fact | **Gap.** Source, version or digest, extractor version and confidence per fact. |
| Bounded, partial queries | `Query` with caps in places, LLM row and byte caps | **Partial.** Needs one stated contract: page size, budgets, cursors, cancellation, partial results. |
| Honest degradation | Source errors, policy gate; search mode not labelled | **Partial.** Label keyword versus semantic search, and stale versus current results. |
| Language-independent queries | Per-language extractors (Rust, Julia, Python) | **Partial.** `tags.scm` and semantic classes would widen coverage cheaply. |
| Agent tools on the same graph | Agent tools over query, fetch and search, with policy and audit | **Strong.** Add status, locate, changed-since and pagination with source/confidence labels. |
| Multi-source graph | Several folders, sources registry; projects designed ([[Projects and Sources]]) | **Direction matches.** Thread's scenarios are useful acceptance tests. |
| Committed vs local, conflict prediction | Git extension, entity log; collaboration is future work ([[Collaboration]]) | **Not now.** Only with a concrete multi-user requirement. |

## What would help, in order
1. **Incremental, persisted index.** A changed source node invalidates the facts extracted from it. Unchanged content (by hash) is skipped. Derived data is stored with the parser and extractor versions, so a version bump invalidates it, and paid embeddings are never recomputed. Keep this separate from Moonkale's own state store unless a measurement says otherwise.
2. **Provenance per fact,** and a visible *stale* or *pending* state while an update runs or after it fails.
3. **One bounded-query contract** for the graph view, the agent tools and remote sources, tested on a large fixture and on the phone.
4. **Agent tools:** index status, locate symbol, changed-since, paginated traversal, all carrying source and confidence.
5. **Visible failure:** not indexed / indexing / current / stale / partial / failed; never an empty graph that looks like success.
6. **Measurements first:** cold open, one-file change, fan-out, index size and memory per platform. Set Moonkale's own targets; Thread's numbers are for other runtimes.

## What should not transfer
- **A dataflow engine in `moonkale-core`.** Incremental scheduling, if built, is an internal part of the index, replaceable and dependency-light.
- **Thread's storage stack** (Postgres, D1, Qdrant, Vectorize) or a second database chosen by analogy. Any index store is evaluated on Moonkale's platforms first.
- **Base + local-delta overlays and conflict prediction** before there is a multi-user requirement.
- **Their targets as promises.** Their own reports disagree, and they measure different runtimes.
- **Any code from Thread** (AGPL, see above).

## Next steps (recommendations, not roadmap)
1. A small Moonkale spec for incremental index invalidation and persistence. Acceptance tests:
   - a one-file change touches only its own facts;
   - reopening a folder reuses the stored index;
   - unchanged content is a no-op;
   - an extractor version bump invalidates its facts;
   - a failure leaves visible stale state.
2. A prototype on `Source::changes_since` and the existing `IndexSource`, with scheduling, extraction and persistence separated.
3. Provenance and the bounded-query envelope specified before new graph facts or agent tools are exposed.

## Sources
- Recoco: `../recoco-main/README.md`, `docs/v1-way-ahead.md`, `site/src/content/docs/guides/architecture.md`, `reference/configuration.md`, `reference/core-crate.md`.
- Thread: `../thread-main/specs/001-realtime-code-graph/` (`spec.md`, `tasks.md`, `data-model.md`, `contracts/streaming-graph.md`, `contracts/websocket-protocol.md`), `docs/architecture/AI_KNOWLEDGE_LAYER_DESIGN.md`, `docs/architecture/SEMANTIC_CLASSIFICATION_SPEC.md`, `README.md`.
- Moonkale: [[Indexing]], [[Graph-Native Model]], [[LLM and RAG]], [[Internal State]], [[Projects and Sources]], [[Status]].
