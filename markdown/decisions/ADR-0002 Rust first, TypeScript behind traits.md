---
tags: [adr]
status: accepted
date: 2026-09-17
---
# ADR-0002 — Rust first, TypeScript behind traits

**Status:** accepted

## Context
Mature text-editing (CodeMirror), WYSIWYG (Milkdown/ProseMirror) and terminal (xterm.js) components exist in TypeScript and have no Rust equivalents of comparable quality today. The brief permits them if they are modular and replaceable.

## Decision
Allow exactly these TS packages, each in its own folder under `packages/js/`, built to one bundle, driven through a Rust trait (`CodeEditorBackend`, `RichTextBackend`, `TerminalBackend`) with a documented message protocol. JS holds no application state and no language knowledge ([[ADR-0008 Rust owns the document, JS is a view]]). Everything else — grid, graph view, flow editor, Typst — is Rust.

## Consequences
- Replacement is a feature flag flip plus deleting a folder; the `PROTOCOL.md` is the acceptance test.
- A JSON hop per keystroke; acceptable (VS Code's extension host and Zed's remote mode pay the same).
- Bundles stay small because languages/LSP come from Rust.
- Keeps the desktop app in a webview until all three are replaced.
