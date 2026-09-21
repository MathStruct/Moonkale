---
title: "Code editor implementations — CodeMirror and the Rust one"
description: Two code editors behind one document model — the CodeMirror bundle and a Rust/Dioxus editor built on dioxus-code-editor — how a document picks one, how to switch, what each can and cannot do, and whether the editor knows the word under the cursor.
tags: [editors, code, architecture, design]
---
From [[Prompt22]] (2026-09-21): build an extension around [`dioxus-code-editor`](https://crates.io/crates/dioxus-code-editor) next to the CodeMirror one, make switching work like the terminals, and answer: *is the code editor aware of the word the cursor is on?* Design decided here, built in [[Milestone 14 - Rust Code Editor]]. Background: [[ADR-0008 Rust owns the document, JS is a view]], [[JS Interop Boundary]], [[JavaScript Inventory]], [[Case Selector]].

## What `dioxus-code-editor` is
A **controlled** Dioxus component (v0.1.2, MIT, by the Dioxus authors): a transparent `<textarea>` for input laid over a highlighted layer rendered by `dioxus-code`, whose highlighter is **tree-sitter compiled to Rust/wasm through `arborium`** — no JavaScript, and it runs in the browser build too. Props: `value`, `language` (`Language::from_slug("rust")`), `theme` (`CodeTheme::system(light, dark)`), `line_numbers`, `read_only`, `oninput(String)`. It gives Moonkale, in Rust, grammars for every [[Core Languages|core language]] — Rust, Julia, Python, C/C++, JavaScript/TypeScript, Go, **Lean, Nix, Typst**, TOML, JSON, YAML, Markdown, SQL, GraphQL — which the CodeMirror bundle only partly has (P-093: Lezer grammars, no Lean/Nix). What it is not: an editor engine. There is no cursor API, no decorations, no completion popup, no fold/gutter marks, no search, no multi-cursor; input is the browser's textarea (undo, selection, IME come from it). Every edit arrives as the **whole text** (the crate diffs internally for incremental highlighting).

## The two extensions

| | `dev.moonkale.editor-code` — CodeMirror | `dev.moonkale.editor-code-native` — Rust |
|---|---|---|
| rendering / input | CodeMirror 6 in the webview (JS bundle, 1 MB) | `dioxus-code-editor`: Dioxus DOM + a textarea |
| highlighting | Lezer / legacy modes for the core languages (P-093) | tree-sitter via `arborium`, all core languages incl. Lean, Nix, Typst |
| edits to Rust | splices (spec 018) | the whole text per input (`Document.text` replaced; dirty as before) |
| LSP (hover, completion, rename, diagnostics, definition, references, code actions) | ✅ | ❌ (no cursor/decoration API to hang them on) |
| wiki-links, presence gutter, wrap toggle, fold | ✅ | ❌ (wrap: textarea `wrap=off`) |
| save / reload / dirty dot / Ctrl+S | ✅ | ✅ (same toolbar) |
| platforms | desktop, web, phone | desktop, web, phone (pure Rust/wasm) |
| tier | core (default) | **opt-in** |

The Rust editor is the JS-free path for *viewing and simple editing* everywhere, and the first place Lean/Nix/Typst highlight. It does not replace CodeMirror until an editor engine exists on the Rust side ([[Rust-native Editor Candidates]]) — that is where LSP features would move.

## Switching, like the terminals
- `editor.implementation` (`codemirror` — default — or `native`; shown under both extensions' rows in the Extensions panel) decides which editor a document opens in when both extensions are enabled. Only one enabled → that one, whatever the setting.
- **Per document**: each editor's toolbar has a switch (*Rust* on the CodeMirror panel, *CodeMirror* on the Rust panel) that moves *this* document to the other editor — the tab is replaced, the document (text, dirty state, undo history of the new editor from scratch) stays open. `Workspace::editor_choice` remembers the choice per document for the window.
- No modal chooser: the terminal chooser is a one-off per *New Terminal*; a document open must stay instant, so the default is the setting and the switch is a click away.
- Markdown source mode (Source | Rich) keeps CodeMirror for now (it embeds the CodeMirror panel directly and needs the `[[` completion); Flow files are their own editor.

## Is the editor aware of the word under the cursor?
Two cursors: the **caret** (where typing goes) and the **mouse pointer** (hover).

**CodeMirror.** The bundle knows both. What reaches Rust today: the caret's *line* (throttled 250 ms, for presence cursors — Milestone 9) and, on hover, the position the tooltip asks the LSP about (`hover { line, col }` → `Workspace` → language server → text back). Milestone 14 adds the **caret column** to that report (`BackendEvent::Cursor { line, col }` was already sent; the panel only used `line`) and a `Workspace::cursor` signal + `Workspace::cursor_word()` — the identifier under the caret, computed in Rust from the document text (`word_at(text, line, col)`) so every extension can read it without asking the editor. Hover-word for the *mouse* stays on demand (the LSP path); a `hover_word` signal is a small addition if a feature needs it.

**Rust editor.** The crate exposes neither caret nor pointer. The caret lives in the textarea; Dioxus's keyboard/mouse events do not carry `selectionStart`. Milestone 14 reads it on caret-moving events (`onkeyup`, `onclick`, `onselect`) with a one-line `document::eval` (`textarea.selectionStart`) — the one JavaScript touch in the JS-free editor, Dioxus's own eval, no bundle — and feeds the same `Workspace::cursor`. A pure-Rust alternative would be tracking the caret from key events, which breaks on mouse clicks, IME and undo; not worth it until the Rust editor engine owns its own caret. Mouse hover: not tracked (no LSP to feed).

So the answer: **CodeMirror yes (and Rust now gets the word); the Rust editor knows the caret through one property read, and nothing about the pointer.**

Why it matters (Daniel, after Milestone 14): the symbol under the cursor is the anchor for *go to definition* and *centre a graph view on it* from a menu or a double/right click — [[024]] — with a five-button mouse as the expected device — [[025]].

## Also in this milestone
The Rust terminal cut its lower rows (Prompt22): the grid measured the container before the panel had its final size and no resize event followed on desktop — fixed by re-measuring on a timer as well as `onresize`, and by anchoring the grid to the bottom so the prompt is the last thing to vanish (P-112).
