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
`webkit_nvidia_workaround()` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` before the first webview when `/proc/driver/nvidia/version` exists, unless the variable is already set or `MOONKALE_KEEP_DMABUF=1`. WebKitGTK's DMA-BUF renderer crashes its web process inside the NVIDIA EGL driver otherwise.
