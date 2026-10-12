---
title: "ADR-0009 — Patches, not snapshots"
tags: [adr]
status: accepted
date: 2026-09-17
---
**Status:** accepted

> [!note] As built (2026-10-11)
> - The op stream has the shape of patches: `Op::WriteText { node, expected: Version, patch: TextPatch }`, `TextPatch::Replace(Vec<Splice>)`, splice offsets in **chars**. The `expected` version check works: a stale write is a `Conflict`.
> - **It carries snapshots.** Every save sends `TextPatch::whole` (`Document::patch`: "a diff-based patch is a drop-in replacement here"), and so do history restore, settings writes, LSP workspace edits and the flow editor's codegen. Nothing produces a smaller patch yet.
> - **Undo is not replay:** it lives in the views ([[ADR-0008 Rust owns the document, JS is a view]], as built).
> - No CRDT, as decided.

## Context
Whole-file saves are slow for large files, lossy for concurrent edits, and useless for undo replay or future collaboration.

## Decision
All text mutations travel as patches inside `core::Transaction` ops with expected versions. Sources apply patches (folder: read-modify-write atomically; databases: as updates).

## Consequences
- Undo/redo = replay.
- A CRDT (e.g. `loro`/`yrs`) can be introduced later behind the same op stream; not adopted now.
- Sources that cannot patch (a read-only DB) return `Unsupported` per op.
