---
title: "Milestone 14 — Implementation Log"
description: What was built for "A Rust code editor" — the dioxus-code-editor extension with tree-sitter highlighting in Rust for every core language, per-document switching between it and CodeMirror, the caret's word as a Workspace signal, and the Rust terminal's cut rows.
tags: [milestone, log, editors]
---
Plan: [[Milestone 14 - Rust Code Editor]]. Design: [[Code Editor Implementations]].

> [!success] Steps 1–5 done (2026-09-21) — the Rust editor opens, highlights, saves and switches; 107 native tests, 37 browser suites
> **Code Editor (Rust)** (`dev.moonkale.editor-code-native`, opt-in) shows a text document in `dioxus-code-editor`: tree-sitter highlighting in Rust/wasm for every core language — Lean, Nix and Typst included, which CodeMirror's bundle never had — with the CodeMirror panel's toolbar (Save, Reload, Ctrl+S, dirty dot) and a *CodeMirror* switch; the CodeMirror panel gets a *Rust* switch; `editor.implementation` picks the default. **The word under the caret** is a Workspace signal now (`cursor`, `cursor_word()`), fed by both editors — CodeMirror already reported line and column, the Rust editor reads the textarea's caret with one property read. Two dependency knots on the way: **one tree-sitter runtime per binary** (the index moved to arborium's, P-113) and **wasm's missing `stderr`** (P-114). The **Rust terminal's cut rows** (P-112) re-measure on a timer and anchor to the bottom.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `packages/editors/code-native`: `NativeCodeExtension` (panels for `editor_for(node) == "native"`, `skipping` markdown/flow like CodeMirror's), `NativeCodePanel` (toolbar, `CodeEditor`, `language_of`, theme by app theme, `Document.text` from `oninput`), `csrc/wasm_stubs.c` + `build.rs` | ✅ 2 unit tests; wasm client links; `code-native.mjs`: `main.rs` opens with 36 tree-sitter tokens, no CodeMirror mounted | [editor-code-native.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code-native/editor-code-native.md) |
| 2 | `Workspace::{editor_choice, editor_for, choose_editor}`, `editor.implementation` (settings + both extensions' settings sections), CodeMirror's `panels()` filter and *Rust* switch, `on_panel_closed` that respects a move | ✅ `code-native.mjs`: *CodeMirror* → `.cm-content` with the edited text, *Rust* → back; `lsp`, `highlight` suites unchanged | |
| 3 | `Workspace::{cursor, set_cursor, cursor_word}` + `word_at` (unit test); CodeMirror passes the column it already had; the Rust panel reads `selectionStart` on `onkeyup/onclick/onselect` (`line_col` from UTF-16 offsets, unit test) and shows `‹word›` in its toolbar | ✅ `code-native.mjs`: `‹u32›` after moving the caret into `u32` | |
| 4 | Rust terminal: 600 ms re-measure timer from `onmounted`, bottom-anchored screen | ✅ `terminal-native.mjs` still passes; the desktop case (P-112) needs Daniel's look — the E2E viewport never showed it | |
| 5 | `moonkale-index` on `arborium-tree-sitter` + `arborium-{rust,julia,python}` | ✅ the three extractor tests and `index_source` pass unchanged | [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/index/index.md) |
| 6 | verify, log, vault, catalogue | ✅ full batch 37/37; this note; [[Extension Catalogue]], [[JavaScript Inventory]], [[Core Languages]], [[Problem Log]] P-112–P-114 | |

## Answer to the question
*Is the code editor aware of the word under the cursor?* **CodeMirror**: yes, and since this milestone Rust is too — `Workspace::cursor_word()` gives the identifier under the caret of the active document, from the (line, column) the bundle reports on every selection change (throttled 250 ms); the *mouse* position stays on demand through the LSP hover path. **Rust editor**: the crate exposes nothing, so the panel reads the textarea's `selectionStart` on caret-moving events (one `document::eval`, no bundle) and feeds the same signal; the pointer is not tracked. Detail in [[Code Editor Implementations]].

## What the user sees
- Extensions → *Code Editor (Rust)* on; its settings row (and the CodeMirror one) has *Which editor opens a text file*. Open `.rs`, `.jl`, `.lean`, `.nix`, `.typ`… — highlighted in the Rust editor; `‹word›` in the toolbar follows the caret; Save/Reload/Ctrl+S as always.
- *CodeMirror* / *Rust* buttons in the toolbars move the open file between the editors (the text and the dirty state carry over; the browser's undo history does not).
- The Rust terminal keeps its prompt in view when the panel changes size.

## Deviations from the plan
1. **The index changed tree-sitter runtimes** — not planned; forced by Cargo's `links` rule. The upside is real (the index parsers compile to wasm), the risk (arborium's forks of the grammars drifting from upstream) is covered by the extractor tests.
2. **wasm needed two C stubs of our own** (`stderr`, `fprintf`) — arborium's sysroot crate is never linked because nothing references it; reported upstream-worthy.
3. **The Rust terminal fix is verified on the web only**; the desktop symptom came from Daniel and is expected to be gone with the timer (the measured height now follows the tile within 0.6 s).
4. **No modal chooser for editors** (the plan's decision): the setting is the default and the toolbar switches per document.

## Numbers
- `editors/code-native`: 330 lines + 15 lines of C; `arborium` grammars for 20 languages add ~2 minutes to a clean build and ~1.8 MB to the wasm client.
- Tokens on `main.rs` (the fixture, 9 lines): 36 highlight spans.
- Tests: 107 native (+3), 37 browser suites (+1).
