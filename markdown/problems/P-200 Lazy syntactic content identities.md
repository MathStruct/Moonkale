---
title: "P-200 — Lazy syntactic content identities"
tags: [problem, index, content-ids]
status: resolved
phase: WP-66
---
**Status:** resolved for the experimental v1 contract, 2026-10-16 · **Related:** [[Code Graph Schema]], [[Indexing]], [[ADR-0026 Memory and the store]]

## Problem
Location ids intentionally change on rename/move and cannot recognize structurally unchanged definitions. A content hash must exclude trivia/local names but keep observable spelling, propagate dependency changes and handle recursion without depending on source order. Eagerly hashing every indexed definition would violate the lazy-work requirement.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | hash raw definition bytes | simple | renames/comments/moves are not handled correctly; dependencies are absent |
| B | versioned syntax normalization with explicit requests and bounded SCC hashing | meets the fixture contract without running code | conservative syntax support and same-file resolution |
| C | complete compiler-backed semantic identities | stronger semantic coverage | requires L3 frontends, external dependencies and dispatch rules |

## Measurements
The v1 three-language fixtures verify rename/move/trivia invariance, observable spelling, body/dependency changes, recursive components, stable unresolved paths, lazy persistent caching and freshness. Cache tests cover byte-budget eviction and corrupt/versioned records; integration tests force a concurrent refresh and malformed/deep/oversized input. Exact final command results are recorded in the Codex handoff; no throughput or semantic-proof claim is made.

## Decision
B, behind the default-off `content-ids` Cargo feature. Keep location ids and source truth unchanged. Return typed errors for unsupported binding scopes rather than inventing identities. Store full BLAKE3 digests beside content UUIDs. Explicitly preserve Python keyword-capable parameter names and define symmetric SCC members as equivalent rather than breaking ties by name or location. The versioned specification is in [[Code Graph Schema]].

## Lesson
An alpha-normalizer needs a binding model, not a textual identifier replacement. Optional doc-only syntax containers must disappear with their docs. Ordered-reference graph canonicalization needs a symmetry rule. Exact input freshness and atomic publication matter as much as hashing; caches are replaceable derived data, never authority.
