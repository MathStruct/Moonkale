---
title: "Milestone 14 — A Rust code editor: the plan"
description: A second code editor extension on dioxus-code-editor (tree-sitter highlighting in Rust for every core language), switching between editors per document and by setting, the word under the cursor as a Workspace signal, and the Rust terminal's cut-off rows. Record in Milestone 14 - Implementation Log.
tags: [milestone, planning, editors]
---
From [[Prompt22]]. Design: [[Code Editor Implementations]]. Record: [[Milestone 14 - Implementation Log]].

## Scope
1. **`packages/editors/code-native`** — `NativeCodeExtension` (`dev.moonkale.editor-code-native`, opt-in): a panel per text document it claims, `dioxus-code-editor` inside, the same toolbar as CodeMirror's (path, dirty dot, language, Save, Reload) plus the *CodeMirror* switch; language from `Node::language_hint` → `Language::from_slug`; theme follows the app theme (`CodeTheme::system`); edits replace `Document.text`; Ctrl+S saves. Grammars: the core languages only (features `lang-*`), not `all-languages`.
2. **Claiming**: `Workspace::editor_for(node) -> "codemirror" | "native"` from the per-document choice (`editor_choice`), else `editor.implementation`, else whichever extension is enabled; both extensions filter `documents` by it; the CodeMirror panel gets a *Rust* switch. Settings under both rows.
3. **Cursor word**: `Workspace::{cursor, set_cursor, cursor_word()}` (line, col of the active document's caret); the CodeMirror panel passes the column it already receives; the native panel reads `selectionStart` on caret-moving events. `word_at` unit-tested.
4. **Rust terminal rows cut** (P-112): re-measure on a timer + anchor to the bottom.
5. Verify (E2E `code-native.mjs`: enable, open `main.rs` in Rust editor, highlighted spans, type + save, switch back to CodeMirror and forward; `cursor_word` checked through the status bar or a data attribute), log, vault, [[Extension Catalogue]], [[JavaScript Inventory]].

## Decisions
- No modal chooser for editors (opening must be instant); the setting decides, the toolbar switches.
- `all-languages` off: ~100 grammars would double compile time; the core list is enough and matches [[Core Languages]].
- The one JS touch in the Rust editor (reading `selectionStart`) is Dioxus's eval, not a bundle; documented in [[JavaScript Inventory]].

## Risks
| risk | mitigation |
|---|---|
| `dioxus-code-editor` 0.1.x churns | pinned; the extension is 300 lines around it |
| big files in a textarea + full re-highlight per keystroke | the crate highlights incrementally; [[Case Selector]] still points big files at plain text |
| the textarea's undo is lost on switching editors | stated in the docs; the document's text is what carries over |
