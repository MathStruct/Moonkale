---
title: "editor-code-native — implementation notes"
tags: [crate-notes, milestone-14]
---
Notes for `moonkale-editor-code-native` (Milestone 14). Design: [[Code Editor Implementations]].

## What it is
`NativeCodeExtension` (`dev.moonkale.editor-code-native`, opt-in): a panel per text document `Workspace::editor_for(node) == "native"`, containing `dioxus_code_editor::CodeEditor` — a textarea over a highlight layer rendered by `dioxus-code`, whose grammars are `arborium` (tree-sitter compiled to Rust and wasm). Only the core languages are enabled (`lang-*` features; `all-languages` would be ~100 grammars). Language from `Node::language_hint` (`language_of`: `shell/bash → bash`, `pixi → toml`, `postgres → sql`), theme by the app theme (`TOKYO_NIGHT` / `GITHUB_LIGHT`). Edits: `oninput` gives the whole text → `Document.text`; Save/Reload/Ctrl+S as in the CodeMirror panel; the *CodeMirror* switch calls `Workspace::choose_editor(node, "codemirror")` and the CodeMirror panel's *Rust* switch the reverse — the tab is replaced, the document stays open (`on_panel_closed` closes the document only if the panel is still ours).

## The caret
`selectionStart` of the textarea, read on `onkeyup`/`onclick`/`onselect` through `document::eval` (the one JavaScript touch — Dioxus's eval, no bundle), converted from UTF-16 units to (line, column) in `line_col`, and handed to `Workspace::set_cursor`; `Workspace::cursor_word()` shows in the toolbar as `‹word›`.

## Two dependency facts
- **One tree-sitter runtime per binary** (P-113): `arborium-tree-sitter` and the `tree-sitter` crate both `links = "tree-sitter"`, so `moonkale-index` moved to `arborium-tree-sitter` + `arborium-{rust,julia,python}` (same `LanguageFn` ABI, same node kinds; the index could now run on wasm).
- **wasm needs `stderr`/`fprintf`** (P-114): tree-sitter's `alloc.c` references them; arborium's own stubs cover malloc/free/…, the rest sits in `arborium-sysroot`, a crate no Rust code references and rustc therefore never links. `build.rs` compiles `csrc/wasm_stubs.c` (two symbols) on wasm32.

## Not there
LSP, decorations, completion, search, fold, wrap, wiki-links — the crate has no cursor/decoration API; those stay with CodeMirror until a Rust editor engine exists ([[Rust-native Editor Candidates]]).

## Tests
`cargo test -p moonkale-editor-code-native` (`line_col`, `language_of`); `packages/web/tests/e2e/code-native.mjs` (open `main.rs` in the Rust editor with tokens, caret word, type + Ctrl+S, switch to CodeMirror and back).
