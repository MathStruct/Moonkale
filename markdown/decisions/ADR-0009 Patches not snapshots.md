---
tags: [adr]
status: accepted
date: 2026-09-17
---
# ADR-0009 — Patches, not snapshots

**Status:** accepted

## Context
Whole-file saves are slow for large files, lossy for concurrent edits, and useless for undo replay or future collaboration.

## Decision
All text mutations travel as patches inside `core::Transaction` ops with expected versions. Sources apply patches (folder: read-modify-write atomically; databases: as updates).

## Consequences
- Undo/redo = replay.
- A CRDT (e.g. `loro`/`yrs`) can be introduced later behind the same op stream; not adopted now.
- Sources that cannot patch (a read-only DB) return `Unsupported` per op.
