---
title: "P-233 — Structural edits need index nodes in the graph"
tags: [problem, sophia, graph, structural-edits]
status: resolved
phase: "ADR-0030 legacy default"
---
**Status:** resolved 2026-10-28 · **Related:** [[ADR-0030 Where the truth lives]], [[ADR-0029 Sophia stores the code graph]], [[Indexing]]

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

Implemented option A (codex, 2026-10-28); the interim setting override is removed.

Interim (Claude, 2026-10-27): both suites set `code_graph.source = in-memory`, so they test the structural edits themselves. Proposed fix: A.

## Lesson

A second graph source with its own ids breaks every feature that keys on index ids. Check the other selection consumers (inspector, Show in Graph, trace) when fixing this.

## Implementation (2026-10-28, codex)

Option A: `Workspace::index_node` calls the owning index's bounded `index-node`
read protocol. One pure function matches native keys first, otherwise exact
file/start/end and the input digest from the current cache manifest. The Sophia
adapter exposes bounded location rows without leaking Sophia types. A nominal
function with one member maps to that member; overloads and content bodies
shared by multiple declarations are refused rather than choosing a file.
Structural controls resolve asynchronously and reject a result for an older
selection. Existing planner input, revision, proposal and rename guards remain.

### Selection-consumer audit

| Consumer | Result |
|---|---|
| Structural controls | Needed mapping; now receive the index node and its provenance. |
| Rename | Uses the same structural controls, then the existing file/revision/LSP guards; no second mapper. |
| Inspector | No separate node inspector exists in this checkout; the debug webview inspector is unrelated. Graph hover/focus reads the shown nodes directly. |
| Show in Graph | Already works: the table sends an explicit source/dialect/query through `GraphRequest`; it never requires an index node. |
| Reveal / double-click | Already works: Sophia text opens through its source; index files keep their existing route. |
| Trace | Already works: trace-family nodes carry file/line/column, resolved through `open_relative_path` and `reveal`. |

Both browser suites now use the default graph. They pin actual Sophia node ids,
click through the renderer and assert the controls' selected index id differs
from the clicked id. The structural suite also checks visible function
preselection: options arriving asynchronously now mark the selected option
explicitly, keeping the dropdown consistent with the review target. Both editor implementations are checked.

## Verification (2026-10-28)

- All 12 AGENTS.md gates pass, including workspace tests/clippy, both distribution
  checks, rustdoc with warnings denied, wasm and fuzz formatting.
- The two pure matching tests cover native-key priority, ambiguity, exact file
  and span endpoints, and stale input rejection. The real folder-cache
  regression covers single-member nominal resolution, distinct Sophia/index
  ids, native-key matching, direct index selections, foreign-cache rejection
  and shared-body refusal (one fixture test: 0.16 s on the final focused run).
- Index and Sophia tests pass with `moonkale-index/sophia`; server and graph
  tests/clippy pass with structural edits, and the web structural wasm check passes.
- CI structural feature set `julia,structural-edits`: `structural-edits` and
  `graph-rename` pass with both native and CodeMirror editors; `group-history`
  and `graph-a11y` pass. No in-memory setting override remains.

- CI Sophia feature set `sophia`: `sophia-folder` passes, completing seven
  browser-suite executions on the final code. The test server was stopped.

Local validation logs are retained under `target/next24-*.log` in the codex
worktree. This is functional coverage, not a large-store latency benchmark.
