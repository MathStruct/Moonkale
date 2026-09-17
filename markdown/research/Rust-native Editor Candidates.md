---
title: "Rust-native Editor Candidates"
tags: [research, interop]
---
What could replace each TS package behind its trait ([[JS Interop Boundary]]).

## Code editor (`CodeEditorBackend`)
| Candidate | Fit | Notes |
|---|---|---|
| Virtualised Dioxus text view | good | rows as DOM lines, only the viewport rendered; contenteditable-free (own cursor/selection); works everywhere. Enough for most files. |
| wgpu text renderer (shared with graph labels) | great perf | needs the canvas surface path; IME/accessibility harder. |
| `helix-view` / `helix-core` | model only | rope + syntax via tree-sitter; rendering is TUI. Could donate the *model* layer. |
| Zed's `gpui` editor | no | tied to gpui, not embeddable in a webview. |
| `lapce`'s `floem` editor | partial | floem is its own UI toolkit. |

Recommendation: Dioxus virtualised view first; it reuses the rope + decorations unchanged.

## Rich text (`RichTextBackend`)
Hardest. No Rust WYSIWYG markdown editor exists. Path: render the block tree with Dioxus, own selection/cursor per block, edit as text-in-block. Long horizon; Milkdown stays for a while.

## Terminal (`TerminalBackend`)
Easiest. `alacritty_terminal` already gives the grid; drawing it in a virtualised Dioxus grid (or wgpu text) is bounded work. First candidate to replace.

## Order
terminal → code → rich text. When all three are done, the desktop can move to `dioxus-native`.
