---
title: "P-227 — Flow code generation could not replace an existing file"
tags: [problem, flow]
status: resolved
phase: WP-05
---
**Status:** resolved · **Related:** [[Flow Editor]], [[Work Packages]], [[Security]]

## Problem
The first Generate created `circuit.jl`, but generating again after adding a
spring/mass left the old RC-only file intact. The panel passed the node listing's
metadata version as the write precondition. Since WP-23, only a fetched content
version is a valid save precondition. The source correctly refused the stale token.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Use the fetched text and version together | Correct patch length and optimistic write precondition | One content fetch, already required |
| B | Remove the precondition | None | Could overwrite concurrent edits; rejected |

## Measurements
Reproduced by `mtk.mjs`'s second Generate (RC to mixed electrical/mechanical
model), 2026-10-23. The generated file retained 29 RC lines rather than the
40-line mixed golden. The first create and parameter editing succeeded.

## Decision
The flow panel now fetches `(text, version)` and uses that version with the
whole-text patch derived from the same text. A failed fetch refuses the write;
there is no zero-length or metadata-version fallback. The existing MTK browser
regression checks the complete replacement byte-for-byte.

## Lesson
Query/listing versions are freshness hints, not content write preconditions.
Keep fetched text, patch length and save version together.
