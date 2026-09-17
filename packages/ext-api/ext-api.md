---
title: "ext-api — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `moonkale-ext-api` (Milestone 1). Design: [[Extension System]], [[Host API Reference]].

## What exists
- `Manifest { id, name }`.
- `PanelContribution { id, title, home, closable, dirty, node }` — the only contribution point. `node` lets the shell map the active document to a tab without knowing any editor.
- `Extension` trait: `manifest()`, `panels(ws)`, `render(panel_id, ws)`, `on_panel_closed(panel_id, ws)`. `panels()` runs inside the shell's render and may read workspace signals — that is how tabs appear/disappear and dirty dots update.
- `Document { node, saved, text, version }` — `dirty()` is `text != saved`; `patch()` is one whole-document splice for now.
- `Workspace` — the host handle. `Copy`; every field is a `Signal` created at `ScopeId::ROOT` so it lives for the app. Operations: `open_folder`, `query`, `open_node`, `close_node`, `save`, `reload`, `set_status`.

## Critical decisions
- **Documents are owned by the workspace**, one `Signal<Document>` each in `documents: Signal<Vec<(NodeId, Signal<Document>)>>`. A keystroke re-renders only readers of that document's signal; opening/closing re-renders readers of the list. Panel remounts (docking) cannot lose text — proven by the E2E split step.
- **`OpenFolder` is a `fn` pointer** (`fn(String) -> OpenFolderFuture`), installed by the platform crate. Desktop passes an in-process `FolderSource` factory, web a `RemoteSource` factory; the shell can't tell them apart.
- **Depends on `dioxus`** because static extensions return `Element`. The WASM path (`ui::Tree`) is not started and will not.
- `Workspace::save` maps a refused op to `SourceError` and leaves the document dirty, so the editor can offer *Reload*.
