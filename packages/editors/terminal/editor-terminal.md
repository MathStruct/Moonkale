---
title: "editor-terminal — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-editor-terminal` (Milestone 3). Design: [[Terminal]]; JS side: [xterm PROTOCOL.md](https://github.com/MathStruct/Moonkale/blob/master/packages/js/xterm/PROTOCOL.md).

- `TerminalExtension` contributes the **terminal** panel (bottom tile of the default layout) and handles `Command::NewTerminal` — from View → New Terminal, Ctrl+`, the panel's `+`, or the Explorer's `>_` ("new terminal here", which sets `ws.terminal_cwd` first).
- `TerminalPanel` shows one tab per session with close buttons; sessions live in ROOT signals so docking the panel elsewhere does not kill shells.
- `SessionView` mounts `window.moonkale.xterm` in a deterministic element (`mk-term-<session>`) from `onmounted`, pumps `Output` chunks to `xterm.write` (base64 → bytes; batched per frame in JS), forwards `onData` to the backend and `onResize` to `resize`. Ctrl+click asks JS for the row text plus neighbours (`lineAt`), runs `links::find`, and opens the file via `ws.open_relative_path`.
- Web: the backend is `api::RemoteTerminal` (websocket `/api/terminal`); desktop: `PtyBackend` in-process.

E2E: `packages/web/tests/e2e/terminal.mjs` (prompt appears, `echo` output, second tab, close, Ctrl+click on `src/main.rs:1:4` opens the file — the click is dispatched synthetically, see P-055).
