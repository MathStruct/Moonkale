---
title: "editor-code-native — implementation notes"
tags: [crate-notes, milestone-14]
---

`NativeCodeExtension` (`dev.moonkale.editor-code-native`, opt-in) delegates to `moonkale_code_view::RustCodeEditorPanel`. Markdown Source uses the same panel when its selected implementation is `native`. CodeMirror remains the default.

The production Rust panel now uses editor-core for text operations, grapheme movement and selection. Dioxus renders only requested viewport rows; a temporary textarea delivers IME/paste text and never supplies the document or selection. Rust syntax highlighting uses `dioxus-code::advanced::Buffer` directly. The `dioxus-code-editor` dependency is no longer used by code-view.

Workspace stores both its revisioned document session and typed view-state slots by `NodeId`. The Rust slot retains the engine, viewport row and handled command sequence across remounts/switches. Localized native deltas become atomic UTF-8 batches. Workspace remains canonical; stale input resynchronizes the engine. Undo/redo restore shared snapshots and UTF-16 selection. Reload/external replacements invalidate history. Source CRLF is preserved although the engine normalizes to LF.

Linux/macOS/Windows desktop clipboard uses the `native-desktop` feature, wired through the desktop app's `desktop` feature. The browser uses native clipboard events and the input sink. Direct OS IME/clipboard checks for the integrated component and Android remain open.

Native views now share LSP sessions and synchronize open/change/save/close notifications. Published diagnostics appear as gutter marks, underlines and clickable messages; Unicode/CRLF coordinates, stale versions and duplicate/remount behavior have deterministic browser coverage. Search/replace and fold controls are also implemented. See [[Rust Code Editor Implementation]] for tests and limitations. Hover information is available on pointer rest and through the toolbar, with cancellation and stale-response checks. Completion supports automatic/manual requests, keyboard/mouse selection and guarded, grouped text/import edits. F12 definition and its toolbar action reveal same-file/folded targets and open cross-file targets with cancellation and guarded loading. F2 rename stages every target before applying unsaved, undoable edits, preserving dirty text and rejecting stale or invalid results. Mod-. code actions support edit/resolve results with diagnostic context, guarded multi-file edits and keyboard/mouse lists. Shift-F12 references list and reveal targets through guarded navigation. Server command execution, full accessibility and platform parity remain incomplete.

Markdown Source now supports resolved/unresolved wiki marks, async `[[` completion with grouped undo and Ctrl/Cmd-click follow/create. Index/text changes refresh the marks; delayed completion/navigation is guarded. Presence initials update in the gutter without LSP. Full accessibility, real-server/native WebView acceptance and remaining platform parity are still open.

Workspace reveals synchronize canonical text before placing the UTF-16 caret. Retained tabs republish cursor/word context on activation, dock moves preserve view state, and close/reopen does not replay old reveals. Cross-window moves support saved tabs; dirty offers are refused and edits made after an offer stay in the origin. Native OS gestures and remaining platform acceptance are still open.

The tree-sitter runtime uses Arborium (the same `links = "tree-sitter"` runtime as the index). The code-view build script supplies missing wasm `stderr`/`fprintf` stubs; editor-core's local fork uses `web_time` for undo timing.


2026-10-15 (WP-03): `editor.implementation = "native"` activates the shipped Rust
adapter unless explicitly disabled. The core Code editor also honors it in
minimal distributions. `settings::DEFAULT_CODE_EDITOR` is still `codemirror`.
Active canonical selections and unsaved documents survive switching; no settings
or source rewrite is required. Rust has bounded live accessibility context,
keyboard focus escape/wiki follow and actual browser/AT-SPI semantic checks.
See [[Rust Code Editor Accessibility and Cutover]] for limits, validation and the
conditional fallback/retirement plan; physical desktop frames and Android remain
gates before the default flip.

NEXT-5 reserves a transparent 2 px surface border and changes its color on focus.
The former ancestor outline caused expensive virtual subtree replacement during
preview scrollbar dragging. The source keeps a visible focus boundary, including
focused hover/tools, and individual controls keep their outlines. Input/help/status
semantics and reduced motion are retained. Focus cannot change layout; geometry
uses the viewport's mounted rectangle inside the reserved border.


WP-69 (2026-10-17): the toolbar is Save, Reload, Wrap and ⋯, using SVG icons,
hidden labels and descriptive tooltips with available shortcuts. The gear is in
⋯ to keep the four-control budget; its existing shell popover now exposes shared
wrap, indentation style and width preferences. Both adapters share this settings
view in `code-view`; the core Code extension contributes the Rust palette routes
so minimal distributions retain them without the optional adapter. Preview, LSP, comments/folding and
the CodeMirror switch are in ⋯ and the command palette. Tab/Enter and touch open
the disclosure; Escape closes it and restores trigger focus. The cursor word is
inline and hidden below 700 px, costing no source row. `toolbar-icons.mjs` and
`code-native.mjs` cover the integrated toolbar; native fixture tests open the
disclosure through `native-toolbar.mjs` before invoking its commands.
