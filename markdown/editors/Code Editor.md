---
title: "Code Editor"
tags: [editor, code]
---
Crate: `editors/code`. Opens any `File{Text}` node; lowest priority so specialised editors (markdown WYSIWYG, flow) win for their kinds, with "Open with…" to override.

Replacement requirements and the current CodeMirror feature/interface survey: [[Code Editor Requirements]].
Rust replacement milestones: [[Rust Code Editor Roadmap]]. M1 code and verification record: [[Rust Code Editor Implementation]].

## Architecture
- **Document in Rust**: `ropey::Rope` + version + undo stack. Edits become `core` patches ([[ADR-0009 Patches not snapshots]]).
- **Backend options**: CodeMirror 6 remains the default via [[JS Interop Boundary]]. An opt-in `RustCodeEditorPanel` now exists in `packages/code-view`; see [[Rust Code Editor Implementation]] for its current limits and verification status.
- **Decorations are data**: `Highlight`, `Fold`, `Diagnostic`, `InlayHint`, `Gutter` produced by [[Indexing]] (tree-sitter) and [[LSP and Terminal]], consumed by any backend. The CodeMirror bundle therefore ships **no language packages**.

## Languages (initial)
| Language | Grammar | LSP | Why first |
|---|---|---|---|
| Rust | tree-sitter-rust | rust-analyzer | dogfooding |
| Julia | tree-sitter-julia | LanguageServer.jl | host of [[Flow Editor]] targets (Lux.jl, MTK) |
| Go | tree-sitter-go | gopls | mature, simple |
| Unison | community grammar (verify) | `ucm` | database-backed codebase = a natural `Source` |
| Lean 4 | tree-sitter-lean (verify) | `lake serve` | infoview as an extra panel |

Each is a `LanguageContribution` in `editors/code/src/languages/`, proving the contribution point works ([[Example - Language]]).

## Features by phase
1. Open/edit/save, highlighting from tree-sitter, folding, search.
2. LSP: diagnostics, hover, completion, go-to-definition (which is a graph edge!), references.
3. Semantic tokens, inlay hints, code actions, format.
4. "Show AST", "show call graph of this function" → open a `GraphView` in the [[Graph View]].

## Big cases
Backend chosen by measured size — see [[Case Selector]] (design).

## Unicode input
`\name` → symbol with a dropdown, in this editor and every other input: [[Unicode Input]] (design, not built).

## Unison note
Unison stores code in a codebase DB, not files. A `UnisonSource` extension (definitions as nodes, dependency edges) would make Moonkale one of the few editors that shows Unison the way Unison thinks. Good showcase; later phase.

## Designs not built yet
Removed from `editors/code/src` as comment-only files in Milestone 18 phase 1:
- **Decorations** (`decorations.rs`) in a neutral form — `Highlight { range, token }`, `Fold`, `Diagnostic`, `InlayHint`, `Gutter` — produced by the index/LSP and consumed by any backend. Today diagnostics and wiki-link marks go to CodeMirror in their own messages, and highlighting is the bundle's (P-093).
- **Document** (`document.rs`): rope + version + undo stack + node id, turning backend edits into `core` patches and applying incoming changes (external change → reconcile or prompt). Today `ext-api::Document` holds a `String`; the Rust editor shares revisioned undo history through Workspace.
- **A Rust-native backend** (`backend/native.rs`): a virtualised Dioxus text view or a wgpu text renderer over the same rope and decorations. The opt-in Rust extension now uses editor-core and a virtualized Dioxus view in `code-view`; neutral decorations and feature parity remain unfinished.
