---
title: "editor-code — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `moonkale-editor-code` (Milestone 1). Design: [[Code Editor]], [[JS Interop Boundary]], [[ADR-0008 Rust owns the document, JS is a view]].

## Pieces
- `backend/mod.rs` — `CodeEditorBackend { set_text, focus }` + `BackendEvent::{Ready, Changed(String)}` + `mount(element_id, initial, on_event)`. Small on purpose; decorations/selection come with the index and LSP.
- `backend/codemirror.rs` — one `document::eval` per editor. The script polls for `window.moonkale.codemirror` (the bundle is a deferred `<script>`), awaits the initial text on `dioxus.recv()`, mounts, then loops: JS → Rust `{kind:"change", text}` via `dioxus.send`, Rust → JS `{kind:"setText"|"focus"|"destroy"}`. A `spawn`ed task pumps `recv()` into the panel's callback; the loop ends when the eval finishes (panel unmounted). `Drop` sends `destroy`.
- `panel.rs` — `CodeEditorPanel { ws, node }`: toolbar (path, dirty dot, language · version, Save, Reload), Ctrl/Cmd+S on the panel root (`onkeydown` bubbles out of CodeMirror), conflict banner, host `div` with id `mk-editor-<uuid>`. Mounts the backend in `use_effect` after first render so the element exists.
- `extension.rs` — `CodeEditorExtension`: one closable panel per open document (`editor:<uuid>`), `on_panel_closed` → `ws.close_node`.
- `assets/codemirror.js` — **built artifact** from `packages/js/codemirror` (`npm run build` there). Committed so `dx` needs no Node.

## Critical decisions
- **Initial text goes over the channel**, not into the script string — no escaping, no size limit in the eval source.
- **Whole-document `change` events.** Simple and correct; O(n) per keystroke. Splices are the planned upgrade and `TextPatch` already accepts them.
- **Panel holds only the backend handle.** Text/version/dirty live in `Workspace`'s `Document`; a remount re-mounts CodeMirror with the current text.
- **No language packages in the bundle** (284 kB: state, view, commands, one-dark). Highlighting will be pushed from Rust as decorations.

## Cross-window drag
No handle in the panel any more: the workbench **tab** is the drag (P-045). `ui::Frame` maps a dragged tab id to the document by the node uuid embedded in it.

## Milestone 3: language server
- `lsp.rs` — `LspManager` (ROOT signals: sessions per `(language, root)`, a starting set, and `diagnostics: HashMap<uri, Vec<Diagnostic>>`). `ensure(ws, language, root)` spawns the transport through `ws.spawn_lsp()` (desktop: stdio; web: websocket relay), pumps `LspEvent`s into `ws.lsp_status` (status bar) and the diagnostics map, and runs `initialize`. One session per language per folder, shared by every editor.
- `panel.rs` — on mount `did_open(file://<root>/<key>)`; `Changed` → `did_change` with a version counter; Save → `did_save`; unmount → `did_close`. An effect pushes this file's diagnostics to the backend. `Hover{id,line,col}` → `session.hover` → `backend.hover_result(id, text)`; `Definition` → same file → `set_cursor`, other file → `ws.open_relative_path` (the target opens; its cursor is not yet positioned).
- `backend/` — `BackendEvent::{Hover, Definition}` and `set_diagnostics / hover_result / set_cursor` commands; JS side documented in `packages/js/codemirror/PROTOCOL.md` (lint gutter, `hoverTooltip`, F12).
- Status without a server: "rust: no language server for rust (install: rustup component add rust-analyzer)".

E2E: `packages/web/tests/e2e/lsp.mjs` (deliberate type error → gutter marker; hover text from rust-analyzer; F12 jumps to `fn add`; fixing the error clears the marker).

## Known limitations
- The `Reload` button pushes text into CodeMirror via `setText`, which also fires `change` — harmless because `Document` is set first.
- If the bundle fails to load, the panel shows "Loading editor…" forever; a timeout + error message belongs with P-032 (release observability).

## Milestone 4
`Workspace::reveal` support: an effect watches `ws.reveal` for this node and, once the view is ready, calls `set_cursor(line, col)` (once per `seq`). Cross-file go-to-definition now opens the target *and* positions the cursor.

## Milestone 5
- `CodeEditorExtension::skipping(fn(&Node) -> bool)` leaves documents to another extension (markdown).
- Text changed outside the view (agent `editor.replace`, reload) is pushed into CodeMirror (`view_text` tracks what the view shows).

## Milestone 7
- Bundle (`packages/js/codemirror`): `@codemirror/search` (`Mod-f`, `Mod-h`, F3…) and `@codemirror/autocomplete` with one async source that asks Rust (`onCompletion(id, line, col)` → `completionResult(el, id, items)`); `F2` → `onRename(line, col, word)`, `Mod-.` → `onCodeActions(range)`, `Shift-F12` → `onReferences`. `setText` now applies the minimal prefix/suffix diff so the cursor survives external edits. Debug the bundle alone with a static page (`cm-test.html` in the E2E working dir).
- `backend/codemirror.rs`: new events `Completion`, `Rename`, `CodeActions` (variant fields renamed camelCase — P-077), `References`; `completion_result`; an unreadable message is logged, not fatal.
- `panel.rs`: completion answered from `LspSession::completion`; rename prompt bar (`.mk-editor-rename`) → `LspSession::rename` → `lsp::apply_workspace_edit`; code actions bar (`.mk-editor-actions[data-count]`, resolve on click); references list (`.mk-editor-refs`, click reveals). `apply_workspace_edit(ws, root, edit)`: open documents take the edit unsaved, closed files are written through the folder with a version check and re-indexed.
- `save_now`: Ctrl+S, Edit → Save and the toolbar button share one path that writes through the source and sends `didSave` (cargo check runs on save — P-080).

## Milestone 9
- Bundle: `onCursor(line, col)` from a throttled (250 ms) selection listener; `setPresence(el, [{line, label}])` drives a gutter (`cm-presence-gutter`, `PresenceMarker` with initials) through a `StateField` of gutter markers.
- Bridge: `BackendEvent::Cursor`, `set_presence`; the panel reports the cursor to `Workspace::set_cursor_line` and pushes other members' lines for this document into the gutter.
