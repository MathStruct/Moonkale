---
title: "P-208 — Graph rename must refuse incomplete file subsets"
tags: [problem, code-graph, rust]
status: resolved
phase: WP-67
---
**Status:** resolved · **Related:** [[Rust Lossless Syntax and Patches]]

## Problem
A graph-side declaration rename does not update references. Reusing LSP Rename
can return edits for several files, but the graph preview/undo contract currently
covers one document. Keeping only the origin's edits would silently break callers.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Apply local subset | small | incomplete refactoring, broken references |
| B | Accept complete single-file plans only | existing guarded history/preview | refuses multi-file rename |
| C | Multi-document proposal | full feature | needs explicit undo and partial-save policy |

## Measurements
Focused unit tests cover exact Unicode/CRLF plans and grouped history, URI aliases,
other-file and version-only targets, stale versions, duplicate/overlapping ranges,
surrogate boundaries, empty plans and budgets. The graph-rename browser suite uses
real rust-analyzer and both existing editor backends; final evidence is in the handoff.

## Decision
Choose B for this slice. Check the complete LSP response before proposing edits;
reject any other-file target, unsupported resource edit or stale response. Keep
A unavailable. C remains future work. No autosave or new source transaction.

## Lesson
A single-document preview may accept only a complete single-document refactoring.
The current document alone is insufficient for async freshness: also check the
server connection/version and open buffers used by the resolver.
