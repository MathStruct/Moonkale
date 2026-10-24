---
title: "P-231 — Sophia folder replay loses generic memberships"
tags: [problem, sophia, index]
status: open
phase: "ADR-0029 gate 4"
---
**Status:** open · **Related:** [[Indexing]], [[ADR-0029 Sophia stores the code graph]]

## Problem

A replay of Lenticulum.jl's real history against Sophia v0.1.0 produces live
names/memberships that disagree with a fresh build of the same tree, even when
the per-file manifests agree. The gate cannot pass with lost or stale facts.

`Store::replace_files` deduplicates new generic members by content digest but
subtracts every entry in an old manifest's member list. Two methods can have
the same normalized identity. Refreshing their file can therefore subtract two
contributions after adding only one, retracting a method that is still present.
The old manifest retains both entries. This is a Sophia-side accounting bug.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Deduplicate old contributions in Sophia, per file/generic/digest | Matches new contribution accounting | Requires a new upstream release and replay |
| B | Rewrite/repair the cache during measurement | None for this gate | Hides the error and invalidates equivalence evidence |

## Measurements

See [[Indexing]] for the full histories, checkpoint differences and timings.
The minimal ignored regression is
`packages/services/index/tests/history_replay.rs::repeated_generic_content_survives_a_file_refresh`.
It adds `f(x::Int) = x` and `f(y::Int) = y`, then adds a comment and refreshes.
Both commits should equal a fresh store; v0.1.0 loses the generic on refresh.

## Decision

Request the Sophia fix, keep the failure visible, and leave gate 4 unmet.
No upstream checkout or replayed repository is modified. The exact request
and reproduction command are in the untracked `HANDOFF-from-codex.md`.
Review generic-to-content transitions too: their old nominal relations can be
left orphaned when generic delta processing sees a Content target and skips it.

## Lesson

File manifests are necessary evidence, but are not sufficient: compare actual
names and live memberships, and enumerate facts on historical subjects too.
