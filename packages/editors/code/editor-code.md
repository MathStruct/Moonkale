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

## Known limitations
- The `Reload` button pushes text into CodeMirror via `setText`, which also fires `change` — harmless because `Document` is set first.
- If the bundle fails to load, the panel shows "Loading editor…" forever; a timeout + error message belongs with P-032 (release observability).
