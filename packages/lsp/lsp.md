---
title: "lsp — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-lsp` (Milestone 3). Design: [[LSP and Terminal]].

Platform-neutral LSP client; compiles on wasm.

- `LspTransport` trait — `send(String)` one JSON-RPC message, `take_incoming()` the stream of messages from the server. Implementations: `moonkale-lsp-local::StdioTransport` (desktop; also on the server) and `api::RemoteLsp` (web, websocket `/api/lsp`).
- `SpawnLsp = fn(language, root) -> LspTransportFuture` — the `WorkspaceConfig` hook.
- `LspSession` (cheap `Rc` clone) — `initialize(root)`, `did_open/did_change/did_close/did_save`, `hover(uri,line,col) → Option<String>` (markdown fences stripped: the tooltip is plain text), `definition(uri,line,col) → Option<Location>`; `pump()` runs the receive loop and emits `LspEvent::{Initialized, Diagnostics{uri, Vec<Diagnostic>}, Status(String), Closed}`. `$/progress` and `window/showMessage` become `Status` (long progress messages are cut at the first `: ` so the status bar stays short). Requests time out per id; unknown server→client requests get `-32601`.
- `Diagnostic` is neutral (`line/col/end_line/end_col`, `severity` as a string, `message`) so editors need no `lsp-types`.

Tests: `cargo test -p moonkale-lsp` (initialize handshake + publishDiagnostics + hover through a fake transport).
