---
title: "P-238 — Sophia patches need the fetched rendering"
tags: [problem, sophia, editing]
status: resolved
phase: SR-150
---
**Status:** resolved in the contributor implementation; 17 source tests, browser acceptance and all gates pass · **Related:** [[Code Graph Schema]], [[Security]]

## Problem

The SR-150 browser scenario fetched `sample.user` referring to `helper`, then
changed `helper` through a second writer. Sophia reprinted the old reference as
a longer content hash without changing `user`'s edit token. Moonkale applied the
editor's whole-text splice, whose end was 49 characters, to the newer rendering.
The remaining hash suffix caused “expected a definition (found `4`)”. The saved
text was refused; no save warning could reach the user.

## Options

| | approach | consequence |
|---|---|---|
| A | Treat any first splice at zero as a whole-document replacement | silently changes valid prefix-patch semantics |
| B | Retain exact fetched bases and version the rendered snapshot | preserves whole and partial patches and Sophia's own conflict checks |

## Measurements

2026-10-29, Sophia v0.1.8, real browser/server Source path. The submitted text
was valid `function user(x::Int64)::Int64 … helper(x) + x … end`; the server's
patch base had changed length. The source regression explicitly rereads the new
rendering before saving the older one through the same shared Source handle.

## Decision

B. Writable source text versions combine the node, Sophia token and exact
printed text as an opaque equality token. A bounded in-memory cache retains
128 bases, at most 2 MiB each and 32 MiB total. A save applies against its fetched
base, checks Sophia's current raw token and passes that token to `write_text`.
A changed referenced name therefore warns; changing the edited name or a
recursive-group member still conflicts. Successful writes acknowledge their
canonical print version, including settings writes.

An evicted unchanged rendering is reconstructed safely. An evicted rendering
that has changed refuses the save and requires reload. The cache is not
persisted, logged, or an authority boundary. Immutable definitions retain their
read-only version behavior; derived folder sources retain their existing file
versions. Tests cover distinct prints, retention bounds, eviction fallback and
concurrent profile/group writes.

## Lesson

An edit token protecting a definition need not identify the bytes of its
rendering. Splice offsets require the fetched text, even when the upstream
write token remains valid.
