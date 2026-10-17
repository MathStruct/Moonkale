---
title: "P-205 — Graph previews must reject stale asynchronous results"
tags: [problem, code-graph, editor]
status: resolved
phase: WP-67
---
**Status:** resolved · **Related:** [[Rust Lossless Syntax and Patches]]

## Problem
Graph locations describe indexed source, while an open document may contain unsaved edits. Parsing also finishes asynchronously, after a selection change, dismissal or document closure. Reusing an ordinal/range from an old snapshot can target another function.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Apply indexed offsets after asynchronous parsing | little state | wrong-target and stale-edit risk |
| B | Snapshot-bound handles, review generations and document validation | explicit recoverable conflicts | stale targets require fresh confirmation |

## Measurements
Native planner tests exercise exact-input handles, Unicode/CRLF edits, moves, invalid input, worker saturation and limits. The feature-enabled structural-edits browser suite drives actual graph selection through remote planning and normal editor history/save/conflict workflows. The handoff records final gate results.

## Decision
B, behind the default-off structural-edits feature. Planning has no write path. Graph provenance may preselect only against matching input; recomputation requires confirming fresh functions. The existing document adapter alone commits edits. Two worker permits and raw/decoded input limits bound the server path.

## Lesson
A graph selection, syntax handle, async response and editor session have distinct lifetimes. Preserve that distinction through preview and final application.
