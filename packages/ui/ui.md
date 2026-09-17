---
title: "ui — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `ui` (Milestone 1). Design: [[Project Structure]], [[ADR-0010 dioxus-workbench for layout]].

## Pieces
- `frame.rs` — `Frame { config, controls, children }`: creates the `Workspace` and extension list once and provides both via context; renders `TitleBar`, the body, and (when `controls` is given) eight invisible resize handles. Global keybindings (`Ctrl+O`, `Ctrl+W`) dispatch on the command bus. `WindowControls` is a bundle of callbacks the desktop crate fills in — `ui` never depends on `dioxus-desktop`.
- `titlebar.rs` — VS Code-style bar: logo, File/Edit/View/Help menus (click to open, hover to switch, backdrop to close), centered window title (`● file — folder — Moonkale`), and minimize/maximize/close on the right when `controls` is present. Empty areas start a window drag; double-click toggles maximize. Menu entries dispatch `Command`s; `Exit` and `Toggle Developer Tools` are desktop-only and call the callbacks directly.
- `shell.rs` — `Shell {}` takes the workspace and extensions from context, owns a *controlled* `layout` signal (so *View → Reset Layout* works) and handles shell-level commands (`OpenFolder` → native dialog via `Workspace::open_folder_dialog`, `ResetLayout`, `About`). On every render it asks each extension for its `PanelContribution`s and builds `dioxus_workbench::Panel`s (`with_closable`, dirty-dot `with_tab_accessory`). Keeps a `panel id → extension index` map for `on_panel_close`. `active_panel` is derived from `ws.active` through the contribution's `node`. Layout: `side [explorer] | main (empty)`; editors have `home = main`.
- `explorer.rs` — `ExplorerExtension`: open-folder form, per-source lazy tree. Tree state (`expanded`, loaded `children`, `error`) lives in signals created at `ScopeId::ROOT` inside the extension, so it survives docking. Directory click → `Query::Children` on first expand; text file click → `ws.open_node`; binary → status message.
- `assets/styling/shell.css`, `explorer.css` — the dark palette pinned on both `.wb-shell` and `.wb-workspace` (P-003).
- `default_extensions()` — `[Explorer, CodeEditor]`; the platform passes this `fn` in `ShellConfig`.

## Session (multi-window)
`Frame` joins the platform's session bus (`ShellConfig::session`) and delivers incoming `SessionMessage`s to `Workspace::handle_message`; while another window is dragging a document, it renders a full-window drop target (`.mk-drop-target`, drop *or click*) whose acceptance calls `Workspace::accept_drop`; once the drag ends without a drop the offer stays as a `.mk-drop-banner` ("Move it here" / dismiss) because OS drags don't reach other windows on every platform (P-044). *View → New Window* dispatches `Command::NewWindow`, handled with `ShellConfig::new_window`. The Explorer lazily loads root trees for sources that arrive from peers.

## Critical decisions
- **Command bus** (`Workspace::commands`, `dispatch(Command)`): menus, keybindings and buttons all go through one `Signal<(seq, Option<Command>)>`; the active editor panel handles `Save/Undo/Redo/CloseEditor`, the shell handles the rest. It is the seed of the command registry in the design (`core::command`).
- **Platform separation for the window**: `ui` renders controls but only ever calls `WindowControls` callbacks; web/mobile pass `None` and get a bar without window buttons. No `cfg` in rendered markup (P-034) — the difference is a prop.
- **`ShellConfig` is always `PartialEq`-equal.** It holds two `fn` pointers set once at startup; comparing them is meaningless (clippy `unpredictable_function_pointer_comparisons`), and the shell must not remount because of them.
- **No `cfg!` in rendered markup** (P-034): fullstack SSR renders on the server, hydration keeps that markup, so any platform-dependent text shows the *server's* variant on web. The placeholder is now platform-neutral.
- The template's `Hero`, `Echo` and the Prompt-1 dummy workbench were removed; `Navbar` stays for the routers.

## Platform wiring (outside this crate)
`web/src/views/home.rs` passes `open_remote` (→ `api::RemoteSource`); `desktop`/`mobile` pass `open_local` (→ `moonkale_project_fs::FolderSource`, blank path = `.`). Nothing else differs.
