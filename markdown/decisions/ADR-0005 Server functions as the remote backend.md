---
title: "ADR-0005 — Server functions as the remote backend"
tags: [adr]
status: accepted
date: 2026-09-17
---
**Status:** accepted

## Context
Web and mobile builds cannot link database drivers, spawn language servers or open PTYs. Users on those platforms still need them.

## Decision
The `api` crate (Dioxus fullstack server functions + websockets) runs the same native crates the desktop uses (`sources-*`, `lsp-local`, `terminal-pty`, `index`) and exposes `query/fetch/apply/subscribe`, LSP and PTY streams per authenticated user. Clients use `RemoteSource` / `WebSocketTransport` / `RemoteBackend`. The server is "desktop without a screen".

## Consequences
- Code reuse is total; the platform split is a deployment decision.
- The server becomes a code-execution service: auth, jail, limits, allow-lists and audit are prerequisites ([[Platform Matrix]]).
- Desktop users can also connect to a team server (shared databases).
