---
title: "terminal — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-terminal` (Milestone 3). Design: [[Terminal]].

The platform-neutral half of the terminal. It compiles on wasm.

- `TerminalBackend` trait — one live shell: `write(bytes)`, `resize(cols, rows)`, `take_output() → Option<Output>` (an unbounded `Vec<u8>` stream taken once by the panel), `close()`. Two implementations: `moonkale-terminal-pty::PtyBackend` (desktop, in-process) and `api::RemoteTerminal` (web, websocket).
- `SpawnTerminal = fn(cwd: Option<String>, cols, rows) -> SpawnTerminalFuture` — the `WorkspaceConfig` hook each platform provides.
- `TerminalMessage` — the wire format shared by the websocket relay and the xterm bundle: `Open{cwd,cols,rows}`, `Input(base64)`, `Output(base64)`, `Resize`, `Exit`. Bytes are base64 so the same JSON crosses the eval channel and the websocket unchanged.
- `session.rs` — `SessionId`, the `Sessions` list the panel keeps in ROOT signals (survives panel remounts when docked/undocked).
- `links::find(line, neighbours) → Vec<Link{path,line,col}>` — `path:line[:col]` detection for Ctrl+click; the neighbours are the rows above and below because xterm wraps long lines (see P-055).

Tests: `cargo test -p moonkale-terminal` (link detection).
