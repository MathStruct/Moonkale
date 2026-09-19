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
`settings.rs` — `SettingsFile` (persisted shape: every field optional, `version`, tolerant `parse`, `overlay`, `push_recent`) and `Settings` (resolved: defaults ← user ← workspace ← env via `Settings::resolve`; `scope_of_llm` for the badge). `Workspace` holds `settings_user`, `settings_workspace`, `settings` (resolved) and `settings_folder`; `load_user_settings` / `update_user_settings` go through `WorkspaceConfig::settings_store` (`SettingsStore { load, save }`), `load_workspace_settings` / `update_workspace_settings` read and write `.moonkale/settings.json` through the folder source (`Query::Text{path}` then `WriteText`/`CreateText`). `open_folder` loads the workspace scope and pushes the path into recent folders; it passes `OpenOptions { embed }` (from `settings.search` + `settings.llm`) to the platform. `WorkspaceConfig::llm` now takes `LlmSettings`; `secret_store` lets the Settings panel store a key on desktop. `Command::{OpenRecent(n), Settings}` added. `WorkspaceConfig::reopen_last_folder` (desktop `true`): `load_user_settings` opens `recent_folders[0]` when nothing is open yet.

## Milestone 6
- `manifest.rs`: `Manifest { id, name, description, optional, default_enabled, permissions }` with `core(..)` (always on), `optional(..)` (on by default, can be disabled), `opt_in(..)` (off by default) and `with_permissions(&[..])`. Explorer/Search/Settings/Code are core; Markdown/Table/Graph/Terminal/Agent optional; Flow and Lux opt-in.
- `settings.rs`: `ExtensionsFile { enabled, disabled, permissions: BTreeMap<id, Vec<permission>> }` (`set_enabled`, overlay merges per id) and the resolved `ExtensionsSettings::{is_enabled(&Manifest), is_enabled_id(id, default), granted(id), has(id, permission)}`. Third-party (wasm) ids default to off and no permissions.
- `flow.rs`: the flow model shared by the flow editor and library extensions — see [flow.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/flow/flow.md). `Extension::flow_libraries()` (default empty) is the contribution point; `Workspace::flow_libraries` holds the enabled ones (the shell keeps it current).
- `workspace.rs`: `WorkspaceConfig::wasm: Option<WasmExtensions { list, run }>` (platform-provided), `Workspace::wasm_extensions` (manifests found for the open folder), `refresh_wasm_extensions()` (after user settings load and on `open_folder`), `run_wasm_command(ext, command, args)` passes the granted permissions from `settings.extensions`. `Command::NewFile(name, template)` backs File → New Flow….

## Milestone 7
- `command.rs`: `CommandContribution { id, title, keybinding }`, `Keybinding::{parse, matches, display}` (`Ctrl` also matches `Meta`; named keys by DOM name), `fuzzy_score` (subsequence with word-start/adjacency bonuses). `Extension::commands(ws)` / `run_command(id, ws)` are the contribution point; `Command::{Palette, QuickOpen, SearchWorkspace}` added. `SettingsFile.keybindings: BTreeMap<id, text>` (empty = unbound) overlays per id.
- `workspace.rs`: `create_dir`, `rename_node` (re-keys open documents and views to the new node id, keeps text/dirty/version, active follows), `delete_node` (closes documents under it), `count_occurrences` / `replace_in_file` (open document → unsaved edit; closed file → `WriteText` with version check + index refresh), `focus_element(id)`, `fs_epoch` (Explorer reloads), `vcs_status` (git decorations), `git()` / `folder_root()`, `WorkspaceConfig::git`.
- `git.rs`: `StatusEntry`, `Commit`, `GitRequest`, `GitResponse`, `parse_status` (porcelain v2, `-z`), `parse_log`, `LOG_FORMAT` — shared by the git extension and `api::git_run`.

## Milestone 8
- `workspace.rs`: `history` signal (loaded from `.moonkale/history.jsonl` with the workspace settings), `record` / `record_as` / `record_event` (append + best-effort rewrite of the file), appends in `save` (Content, with `base` for files the log never saw and the agent actor from `pending_actor`), `create_text`/`create_dir` (Add), `rename_node` (Rename), `delete_node` (Remove); `user_actor()` from `settings.user_name`. Presence: `presence` signal, `join_presence(room)` on folder open through `WorkspaceConfig::presence`, `publish_presence()`, `my_presence()`, `others()`. Browser wasm: `WorkspaceConfig::wasm_module_url`; `run_wasm_command` tries `run_wasm_in_browser` (an eval driving `window.moonkale.wasmHost`, host calls answered by `answer_host_call` over this workspace's sources with the host's permission check) and falls back to the platform runner.
- `presence.rs`: `Member { window, name, active }` (+ `initials()`), `PresenceMessage::{Join, Update, Members}`, `PresenceLink`, `JoinPresence`.
- `settings.rs`: `user_name` (default `$USER`, "you" in the browser).
