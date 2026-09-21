---
title: "desktop — implementation notes"
tags: [crate-notes]
---
Notes for the `desktop` crate. Everything that touches `dioxus::desktop` lives in `src/main.rs`; `ui` only receives callbacks.

## Window
- **Undecorated** (`WindowBuilder::with_decorations(false)`): the OS title bar is gone and `ui::TitleBar` draws menus + minimize/maximize/close in one row, VS Code style. `Config::with_menu(None)` also removes dioxus-desktop's default native menu bar (its "Toggle Developer Tools" entry is re-exposed under *View* in debug builds via `window().devtool()`).
- `WindowControls` maps to `dioxus::desktop::window()`: `set_minimized`, `toggle_maximized`, `close`, `drag` (mousedown on empty title-bar area), and `drag_resize_window(ResizeDirection)` for the eight invisible edge handles `ui::Frame` renders. Double-click on the bar toggles maximize.
- Known gaps: the maximize icon does not flip to "restore" (no maximize-state polling yet); no window snapping hints; Wayland compositors decide whether `drag_resize_window` is honoured.

## Multiple windows
`new_window()` spawns another `App` via `window().new_window(VirtualDom::new(App), window_config())`. All windows run on the main thread, so the session bus is a `thread_local!` list of `(WindowId, UnboundedSender)`; each window spawns a task that feeds its receiver into `Workspace::handle_message`. Sources are shared through a process-wide `SourceRegistry` (`attach_local` looks a descriptor up there), so a folder is the same `FolderSource` instance — same ids and version checks — in every window; `session()` seeds a new window with every registered source before it says `Hello`, so folder sync does not depend on the bus (P-045). Dragging an editor's path label from one window and dropping it on another moves the document. Whether WebKitGTK delivers the HTML5 drop across two webviews of one process is **unverified here** (no display); if it doesn't, releasing the drag leaves a "Move it here" banner in the other window, so the move still takes one click (P-044).

## Folder dialog
`pick_folder()` uses `rfd::AsyncFileDialog::pick_folder()` with the **`xdg-portal`** backend — the same one `dioxus-desktop` compiles rfd with, so no second dialog toolkit is linked. It talks to the desktop's portal daemon (`xdg-desktop-portal` + a backend such as `xdg-desktop-portal-gtk`/`-kde`/`-hyprland`). Without a portal the future resolves to `None` and the status bar says so; the Explorer's text field remains as the fallback. The dialog is wired to *File → Open Folder…*, `Ctrl+O`, and the Explorer's "Open Folder…" button.

## Why "Open" didn't show a dialog before
Milestone 1 deliberately shipped a text field on every platform (the plan listed the native picker as out of scope). This crate now provides the picker through `WorkspaceConfig::pick_folder`; web and mobile still pass `None`.

## P-061 (Milestone 3)
WebKitGTK's web process crashes inside the NVIDIA EGL driver while tearing down a live WebGL context (our graph renderer) — on every exit, graceful or not, and on every `dx serve` rebuild. `mute_webkit_exit_dumps()` sets the soft `RLIMIT_CORE` to 0 before launch when `/proc/driver/nvidia/version` exists, so the WebKit children (which inherit it) leave no dump and no crash popup. `MOONKALE_COREDUMPS=1` keeps dumps for debugging a real crash. `WEBKIT_DISABLE_DMABUF_RENDERER` and a JS `pagehide` context loss were tried first and did not help.

## Milestone 6
- `mod wasm_ext { list, run }`: an in-process `moonkale_ext_host::Runtime` (feature `wasmtime`) over the registry — `discover(config_dir, folder)` on every `list`, permissions passed per `run`. `WorkspaceConfig::wasm` is set; the module directory is `~/.config/moonkale/extensions` (`MOONKALE_CONFIG_DIR` overrides).

## Milestone 9
- `presence.rs`: the hub client. `MOONKALE_HUB=http(s)://host:port` → `ws(s)://…/api/presence`; `Authorization: Bearer $MOONKALE_TOKEN` when set; `Join` on connect, `Update`s from a channel, `Members` lists back to the UI thread through a channel drained by a spawned future. Frames are JSON in binary websocket messages (what dioxus typed websockets send; text accepted too). `tests/hub.rs` (ignored) joins a running hub and checks the reply — run with `MOONKALE_HUB=http://127.0.0.1:8090 cargo test -p desktop --test hub -- --ignored`.
- `open_database`: `.duckdb` files and CSV/TSV/Parquet files (their folder) open as `DuckDbSource`.

## Milestone 11 — remote
- `MOONKALE_REMOTE=http://host:port` (+ `MOONKALE_TOKEN`) at start → `api::client::connect`; every platform callback (`open_folder`, `attach_source`, `spawn_terminal`, `compile_typst`, `spawn_lsp`, `git_local`, `wasm_ext::{list_any, run_any}`) dispatches on `api::client::active()`. The LLM provider stays local.
- `open_remote(host, path, sink)` → `moonkale_remote::SshSession::open` with `server_binary()`; `RemoteHandle` implements `ui::remote::RemoteSession` (close kills the master `ssh`). `ssh_hosts()` reads `Host` aliases (no wildcards, no `!`) from `~/.ssh/config` for the dialog (`ssh_hosts_in` is unit-tested). `ssh_at_start()`: `MOONKALE_SSH=host:/path` or `--ssh host:/path` opens the remote folder when the window starts (before the recent-folder reopen). `WorkspaceConfig::remote = Some(RemoteHosts { open, hosts, at_start })`.
- `tests/remote.rs` (ignored): the client wiring against a running server.
