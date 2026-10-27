---
title: "P-233 — Structural edits need index nodes in the graph"
tags: [problem, sophia, graph, structural-edits]
status: open
phase: "ADR-0030 legacy default"
---
**Status:** open · **Related:** [[ADR-0030 Where the truth lives]], [[ADR-0029 Sophia stores the code graph]], [[Indexing]]

## Problem

Since NEXT-20 (2026-10-27) the graph in a legacy folder reads the Sophia cache. Its definition nodes come from the `sophia:` source and have their own ids. The experimental structural edits (Rust function edit and rename, behind `structural-edits`) select the clicked node by its index id and plan the edit on the index node. Clicking a Sophia node therefore selects nothing the structural controls know, and the browser suites `structural-edits` and `graph-rename` failed on CI (master 54f74643).

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | map a Sophia definition node to its index node (same file and span, or the native key) when the structural controls take a selection | structural edits work on the default graph | one more mapping to keep in step |
| B | let structural edits work on Sophia nodes directly | one id space | needs the SPEC-010 write path, which is for store-truth folders |
| C | keep structural edits on the in-memory index graph | no work | the feature silently needs a non-default setting |

## Measurements

Local run with CI's features `julia,structural-edits`: both suites fail on the Sophia graph and pass with `code_graph.source = in-memory`.

## Decision

Interim (Claude, 2026-10-27): both suites set `code_graph.source = in-memory`, so they test the structural edits themselves. Proposed fix: A.

## Lesson

A second graph source with its own ids breaks every feature that keys on index ids. Check the other selection consumers (inspector, Show in Graph, trace) when fixing this.
