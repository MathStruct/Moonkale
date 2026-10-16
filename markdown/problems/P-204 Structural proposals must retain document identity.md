---
title: "P-204 — Structural proposals must retain document identity"
tags: [problem, editor, code-graph]
status: resolved
phase: WP-67
---
**Status:** resolved · **Related:** [[Rust Lossless Syntax and Patches]]

## Problem
A pure syntax plan guards source bytes but does not identify an editor session. Closing and reopening the same file, or editing and undoing back to the same bytes, must invalidate pending graph edits. A source can also be replaced or become read-only while a preview is open.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Apply directly to Document text after checking its hash | simple | bypasses history and session identity |
| B | Capture session/source identity, revision and version; use EditBatch | existing history and recoverable conflicts | consumer must explicitly replan |

## Measurements
`ext-api/tests/document_proposals.rs` uses a real Dioxus runtime and actual lossless Rust rename/move plans. It covers revision and close/reopen ABA, backing version, canonical drift, replaced sources, writable Files authority, Unicode/CRLF selection/history, invalid edits, bounds and retained dirty buffers after a version-conflicting save.

## Decision
B: engine-neutral, bounded single-document proposals with immutable base/preview and typed conflicts. No silent rebase or autosave. Consumers retain failed proposals and explicitly capture/recompute or discard. Graph controls and visible conflict presentation remain the next slice.

## Lesson
Exact input bytes, editor revision, session identity and backing source version answer different freshness questions. Check them together immediately before committing, without awaiting between checks and mutation.
