---
title: "editor-terminal-native — implementation notes"
tags: [crate-notes, milestone-12]
---
Notes for `moonkale-editor-terminal-native` (Milestone 12). The JavaScript-free twin of `editors/terminal`: same `SpawnTerminal` backends (PTY on desktop and server, websocket on web), its own panel (`terminal-native`, activity "Terminal (Rust)", opt-in tier).

## How it draws
- One `vt100::Parser` (24×80, 5 000 lines of scrollback) per session; the output pump (started from `onmounted`, P-047) feeds every chunk to `process` and bumps a `frame` signal; the view reads `frame()` and rebuilds the rows.
- `rows_of(screen)`: for each visible row, runs of cells with one style — 16-colour indices as classes (`mk-tn-fg1`, `mk-tn-bg4`), 256/RGB colours inline (`idx_rgb` is the xterm cube + greys), bold/dim/italic/underline/inverse as classes, wide-character continuations skipped, the cursor cell `mk-tn-cursor` (hidden when the screen is scrolled back or the panel has no focus). Trailing blank runs are dropped.
- Size: a hidden 10-`M` probe span and the container are measured with `MountedData::get_client_rect` on mount and on `onresize`; cols/rows = floor((size − padding) / cell); `screen_mut().set_size` + `backend.resize`.

## Input
`keys::encode(key, ctrl, alt, shift, app_cursor)`: printable characters as typed (UTF-8), Enter/Tab/Backspace/Escape/Delete/Insert, arrows and Home/End honouring application-cursor mode, PgUp/PgDn, F1–F12, Ctrl+letter → C0, Ctrl+[\]^_ and digits, Alt → ESC prefix. Modifier-only, dead and IME keys give `None` (and are left to the browser); Ctrl+Shift combos are not intercepted (copy/paste). Typing at a scrolled-back screen jumps to the bottom. Wheel: pixels/lines/pages → `set_scrollback`. Ctrl+click on a row: `moonkale_terminal::links::find` on that row's text → `Workspace::open_relative_path`.

## Not yet
Mouse selection and copy, paste, search, Sixel/images, mouse reporting to the program, IME composition. Until then the xterm.js panel stays the default; `terminal.implementation` and the chooser in `ui/terminal_chooser.rs` pick.

## Tests
`cargo test -p moonkale-editor-terminal-native` (key table); `packages/web/tests/e2e/terminal-native.mjs` (open with Ctrl+`, prompt, `echo`, colours and bold, Ctrl+click on `src/main.rs:2:1`, the chooser with `ask`).

## Milestone 14 (P-112)
The panel re-measures every 600 ms (`sleep_ms` timer from `onmounted`) besides `onresize`, because the webview did not report a tile that changed size after mount, leaving the grid sized for the old height; and `.mk-tn-screen` is a bottom-anchored flex column, so an oversized grid loses its top rows, never the prompt.
