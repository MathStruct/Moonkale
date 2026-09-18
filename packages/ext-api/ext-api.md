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

- `session.rs` — `WindowId`, `SessionMessage` (`Hello`, `SourceOpened`, `DragStarted/Ended`, `Moved`), `trait SessionBus`. `Workspace` gains `window`, `foreign_drag`, `own_drag`, `connect_bus`, `handle_message`, `attach_source`, `start_drag/end_drag/accept_drop`, and `WorkspaceConfig::attach_source`. This is the seed of presence/collaboration (vault `architecture/Collaboration.md`).

## Critical decisions
- **`Version` serialises as a hex string** (P-041): 64-bit hashes don't survive a JavaScript `Number`.
- **`SessionMessage::sender()`** is what echo-suppression keys on; for `Moved` the sender is `to` (P-042).
- **Documents are owned by the workspace**, one `Signal<Document>` each in `documents: Signal<Vec<(NodeId, Signal<Document>)>>`. A keystroke re-renders only readers of that document's signal; opening/closing re-renders readers of the list. Panel remounts (docking) cannot lose text — proven by the E2E split step.
- **`OpenFolder` is a `fn` pointer** (`fn(String) -> OpenFolderFuture`), installed by the platform crate. Desktop passes an in-process `FolderSource` factory, web a `RemoteSource` factory; the shell can't tell them apart.
- **Depends on `dioxus`** because static extensions return `Element`. The WASM path (`ui::Tree`) is not started and will not.
- `Workspace::save` maps a refused op to `SourceError` and leaves the document dirty, so the editor can offer *Reload*.

## Milestone 4
- `WorkspaceConfig::llm: Option<LlmProvider>` (`fn() -> Future<Arc<dyn Provider>>`; `None` on mobile) and `Workspace::llm()`.
- `Workspace::reveal(node, line, col)` opens a node and sets `reveal: Signal<Option<Reveal>>` (with a `seq`); the code editor places the cursor when its view is ready. Used by search hits, trace frames, cross-file go-to-definition and the `editor.open` tool.
- `Workspace::create_text(source, parent, name, text)` → `Op::CreateText` on the source, refresh of derived sources, `graph_epoch` bump (transcripts).
- `Workspace::add_source(Arc<dyn Source>)` for in-process sources (traces) and `next_unique()` for their ids.

## Milestone 5: settings
`settings.rs` — `SettingsFile` (persisted shape: every field optional, `version`, tolerant `parse`, `overlay`, `push_recent`) and `Settings` (resolved: defaults ← user ← workspace ← env via `Settings::resolve`; `scope_of_llm` for the badge). `Workspace` holds `settings_user`, `settings_workspace`, `settings` (resolved) and `settings_folder`; `load_user_settings` / `update_user_settings` go through `WorkspaceConfig::settings_store` (`SettingsStore { load, save }`), `load_workspace_settings` / `update_workspace_settings` read and write `.moonkale/settings.json` through the folder source (`Query::Text{path}` then `WriteText`/`CreateText`). `open_folder` loads the workspace scope and pushes the path into recent folders; it passes `OpenOptions { embed }` (from `settings.search` + `settings.llm`) to the platform. `WorkspaceConfig::llm` now takes `LlmSettings`; `secret_store` lets the Settings panel store a key on desktop. `Command::{OpenRecent(n), Settings}` added.
