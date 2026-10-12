---
title: "ADR-0008 — Rust owns the document; JS is a view"
tags: [adr]
status: accepted
date: 2026-09-17
---
**Status:** accepted

> [!note] As built (2026-10-11)
> - **No rope.** The document is `ext-api::Document { saved: String, text: String, version }`: two strings. `ropey` is not a dependency.
> - **Undo lives in the views**, not in Rust: CodeMirror's `history()` and Milkdown's ProseMirror history. Rust only forwards Undo/Redo commands (`code-view/src/panel.rs`). The Rust code editor on the `codex` branch is the exception: its undo is in Rust (`ext-api::editor::RevisionedDocument`, 100 transactions), and an external change (reload, agent edit, the other editor) clears it.
> - **Edits in:** CodeMirror sends splices in UTF-16 units (spec [[018]]). Milkdown sends the whole document on every change (`RichEvent::Changed(String)`).
> - **"Same for the markdown block tree"** did not happen: there is no Rust-side block tree. Milkdown's ProseMirror document is the tree, and markdown is its import/export format ([[ADR-0017 Markdown live preview on the code editor]] is the way back). The xterm terminal keeps its grid in JavaScript; the Rust terminal (Milestone 12) keeps it in Rust.
> - **Holds:** the text the workspace saves is Rust's, and agents edit files headlessly through the sources, without an editor.

## Context
If CodeMirror's `EditorState` were the source of truth, replacing CodeMirror would mean rewriting document handling, undo, LSP sync and link extraction.

## Decision
The document (`ropey::Rope`, version, undo) lives in Rust. Backends receive text + decorations and emit edits. Same for the markdown block tree and the terminal grid.

## Consequences
- Backend swap touches one module.
- Two copies of the text (Rust + JS) — memory cost acceptable; JS applies edits optimistically and Rust reconciles.
- Enables headless operations (agents editing files with no editor open) trivially.
