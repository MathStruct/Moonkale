---
tags: [adr]
status: accepted
date: 2026-09-17
---
# ADR-0008 — Rust owns the document; JS is a view

**Status:** accepted

## Context
If CodeMirror's `EditorState` were the source of truth, replacing CodeMirror would mean rewriting document handling, undo, LSP sync and link extraction.

## Decision
The document (`ropey::Rope`, version, undo) lives in Rust. Backends receive text + decorations and emit edits. Same for the markdown block tree and the terminal grid.

## Consequences
- Backend swap touches one module.
- Two copies of the text (Rust + JS) — memory cost acceptable; JS applies edits optimistically and Rust reconciles.
- Enables headless operations (agents editing files with no editor open) trivially.
