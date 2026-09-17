---
title: "ui — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `ui` (Milestone 1). Design: [[Project Structure]], [[ADR-0010 dioxus-workbench for layout]].

## Pieces
- `shell.rs` — `Shell { config: ShellConfig }`. Creates the `Workspace` and the extension list once (`use_hook`), then on every render asks each extension for its `PanelContribution`s and builds `dioxus_workbench::Panel`s (`with_closable`, dirty-dot `with_tab_accessory`). Keeps a `panel id → extension index` map for `on_panel_close`. `active_panel` is derived from `ws.active` through the contribution's `node`. Layout: `side [explorer] | main (empty)`; editors have `home = main`.
- `explorer.rs` — `ExplorerExtension`: open-folder form, per-source lazy tree. Tree state (`expanded`, loaded `children`, `error`) lives in signals created at `ScopeId::ROOT` inside the extension, so it survives docking. Directory click → `Query::Children` on first expand; text file click → `ws.open_node`; binary → status message.
- `assets/styling/shell.css`, `explorer.css` — the dark palette pinned on both `.wb-shell` and `.wb-workspace` (P-003).
- `default_extensions()` — `[Explorer, CodeEditor]`; the platform passes this `fn` in `ShellConfig`.

## Critical decisions
- **`ShellConfig` is always `PartialEq`-equal.** It holds two `fn` pointers set once at startup; comparing them is meaningless (clippy `unpredictable_function_pointer_comparisons`), and the shell must not remount because of them.
- **No `cfg!` in rendered markup** (P-034): fullstack SSR renders on the server, hydration keeps that markup, so any platform-dependent text shows the *server's* variant on web. The placeholder is now platform-neutral.
- The template's `Hero`, `Echo` and the Prompt-1 dummy workbench were removed; `Navbar` stays for the routers.

## Platform wiring (outside this crate)
`web/src/views/home.rs` passes `open_remote` (→ `api::RemoteSource`); `desktop`/`mobile` pass `open_local` (→ `moonkale_project_fs::FolderSource`, blank path = `.`). Nothing else differs.
