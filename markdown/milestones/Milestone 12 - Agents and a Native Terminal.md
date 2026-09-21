---
title: "Milestone 12 — Agents and a native terminal: the plan"
description: Claude Code as one of Moonkale's agents (on the subscription, no API key); agent sessions that live on the server and survive every client; the first JavaScript-free editor — a Rust/Dioxus terminal next to the xterm.js one, with a chooser; an Extensions button in the activity bar. Record in Milestone 12 - Implementation Log.
tags: [milestone, planning, agents, terminal, extensions]
---
From [[Prompt20]] (2026-09-21), four requests:

1. **Add Claude Code to the suggested agents.**
2. **A server-run agent keeps running when no client is attached** — no web page, no desktop, no phone — and a client that connects later (the phone over the remote path) sees the current state.
3. **The first JS/TS extension to get a Rust twin: the terminal.** A second terminal extension, pure Rust/Dioxus; when both are enabled, a menu lets the user choose which one opens a terminal.
4. **An Extensions button in the left panel.**

Record: [[Milestone 12 - Implementation Log]]. Design that this builds on: [[Claude Code Extension]] (three levels; this milestone is Level 3, the provider), [[LLM and RAG]], [[JavaScript Inventory]] (the JS-free path), [[Remote and Server Modes]] (the phone reaches a server as a client), [[Extension Catalogue]].

## Starting point (after Milestone 11)
- `moonkale-llm`: `Provider { complete(Request) -> EventStream, embed }` with `anthropic`, `openai`, `ollama`, `mock`; the web client's `RemoteProvider` relays a completion over a websocket to the server, which holds the keys. The **agent loop** (`Agent::send`: tool rounds, policy gate, audit) runs **in the client** — in the browser page or the desktop process — so a closed page ends the turn.
- The Agent panel keeps its `Chat` in root-scope signals (survives panel remounts, not the page).
- `claude` 2.1.278 is installed here; `claude -p --output-format stream-json` speaks JSON lines; the subscription login lives in `~/.claude/`.
- `moonkale-terminal` models sessions and links; the VT grid was left as a note; xterm.js renders. `api::client` + the relay let the desktop be a client of any server (Milestone 11); the mobile app has the same crates but no way to connect yet.
- Settings → Extensions lists every built-in and wasm extension with toggles and permissions; there is no activity-bar entry for it.

## Scope: what "done" means
1. **`claude-code` provider** (`moonkale-llm`, feature `claude-code`, native only): spawns `claude -p --output-format stream-json --verbose` per turn with `--session-id`/`--resume` for continuity, in the open folder (`Request.cwd`), with `permission_mode` (`plan` by default — read-only; `acceptEdits`; `bypassPermissions` only when the user says so) and `allowed_tools` from the settings; the CLI's own tool activity shows in the transcript as compact lines; `result` → `Done` with usage. Claude Code runs its **own** tools inside the folder, so the Moonkale agent loop runs no tool rounds for it. In the provider list of Settings → LLM as *Claude Code (subscription, no key)*; on the server the same provider (the folder is the jail). A **mock `claude`** (a shell script speaking stream-json) for tests.
2. **Server-side agent sessions**: the server owns `AgentSession { id, folder, transcript, running, pending approvals }` in memory and on disk (`<folder>/.moonkale/chats/<id>.jsonl`); server functions `agent_send`, `agent_events(since)`, `agent_list`, `agent_approve`; a turn runs in a tokio task with a **server-side tool host** (the read-only MCP tools over the server's registry; writes wait for an approval from any client, with the policy's `allow_writes` as the auto-yes) and finishes whether or not a client is watching. The Agent panel uses server sessions whenever the workspace's sources are a server's (web client; desktop in remote mode; the phone once connected) — same items, same approvals, plus a session list to reopen one.
3. **Connect to Server…** (desktop **and mobile**): URL + token → `api::client::connect` through the relay; the phone thereby sees the running Claude Code session's state — the concrete case in the prompt. (The remote *folder over ssh* stays desktop-only: the phone has no `ssh`.)
4. **`moonkale-editor-terminal-native`**: a terminal panel that renders a VT grid in Dioxus — `vt100` for the screen model (pure Rust, wasm-clean), rows of styled spans, cursor, scrollback, keyboard → bytes (printable, Enter, Backspace, Tab, Esc, arrows, Home/End, PgUp/PgDn, Ctrl+letter, Alt+key), resize from the container's size via `MountedData::get_client_rect` (Dioxus, no JS of ours), file links from `moonkale_terminal::links`, Ctrl+click opens. Same backends (`SpawnTerminal`: PTY or websocket), same `Sessions` shape, its own `PanelContribution` (`terminal-native`, activity "Terminal (Rust)"). Default tier: **opt-in** until it matches xterm.js; the JS one stays default.
5. **The chooser**: `terminal.implementation = ask | xterm | native`. *View → New Terminal* (and the palette, Ctrl+`) goes to the one implementation when only one is enabled or the setting names one; with both enabled and `ask`, a small dialog: *xterm.js* / *Rust* with *remember my choice*. Each extension answers `Command::NewTerminalIn(<id>)`.
6. **Extensions activity**: an *Extensions* panel (activity icon `puzzle`, order after Settings) with the same content as Settings → Extensions — built-ins by tier with toggles and permissions, installed wasm modules, and a link to the catalogue page; `View → Show Extensions`, palette `view.panel.extensions`.
7. Verify, log, vault; [[Extension Catalogue]] updated (two new extensions, the provider).

**Deferred**: Level 2 of [[Claude Code Extension]] (the IDE bridge: lock file, WebSocket MCP server, `openDiff`), `--permission-prompt-tool` through Moonkale's gate (v1 uses `permission_mode` + `allowed_tools`), sessions as index nodes, mouse selection and search in the native terminal, replacing xterm.js as the default.

## Architecture decisions
- **Claude Code is a provider, not a second agent loop.** The `Provider` trait is the seam the panel, the relay, the audit and the transcript already use; a provider that never emits `ToolUse` makes `Agent::send` a single round. The CLI's tool calls are text in the transcript (`▸ Read src/main.rs`), not `Item::Tool` — those are Moonkale's own tools with Moonkale's approvals.
- **Continuity by `--resume`.** The provider keeps `(session_id, user turns seen)` per folder; one new user message → `--resume`; a reset chat → a new session id (older history, if any, goes into the first prompt as context). The CLI's JSONL under `~/.claude/projects/` is the transcript of record.
- **`Request.cwd`** (new, `serde(default)`): where a turn runs. The server refuses a `cwd` outside `MOONKALE_ROOT`.
- **Sessions live where the sources live.** Local sources → the in-process agent (as today; a desktop turn already survives a hidden panel). Server sources → the server's session store; clients poll `agent_events(since)` (1 s while running), the same shape the panel renders. No websocket per session: polling is what survives phone sleep and tunnel hiccups.
- **Approvals without a client** wait: the turn blocks on a pending approval (with a 10-minute timeout → denied) and the next client to connect sees it. `policy.allow_writes` (workspace settings) auto-approves as it does locally.
- **The native terminal is a second extension, not a feature flag of the first** — that is the JS-free rule of [[JS Interop Boundary]]: replacement = a crate you can enable, the bundle stays untouched until it is deleted.
- **`vt100`** over `alacritty_terminal`: 60 KB, no platform code, already wasm-clean; scrollback, colours, alternate screen, resize. If it falls short (Sixel, some mouse modes), `vte` + own grid later.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | Extensions panel + activity; `View → Show Extensions` | ui | E2E `extensions.mjs`: the rail entry, the panel lists opt-in extensions, toggling one persists |
| 2 | `Request.cwd`; `claude_code.rs` provider (spawn, stream-json codec, session map, settings `claude.{path,permission_mode,allowed_tools}`); provider list + Settings; mock `claude` script | llm, ui, desktop, api | native tests for the codec and the session map against the mock; E2E `claude-code.mjs` on the server with `MOONKALE_CLAUDE_BIN=<mock>` |
| 3 | `api::agent_sessions` (store, disk log, server tool host over the registry, approvals); server functions; panel mode switch + session list; desktop remote mode uses it too | api, agent, ext-api | E2E: start a turn, close the page, reopen → the finished transcript is there; approval from a second tab |
| 4 | *Connect to Server…* (URL + token) on desktop and mobile; status item; disconnect | ui, desktop, mobile | desktop test against the E2E token server; Android: connect to the dev machine's server over the LAN (token, `MOONKALE_INSECURE_HTTP=1` on the wire) and see a session |
| 5 | `editors/terminal-native`: `vt100` grid → Dioxus rows, keys, resize, links; opt-in extension | terminal-native, ui | E2E `terminal-native.mjs`: enable, open, `echo`, colours, resize, Ctrl+click on a path; the same checks `terminal.mjs` does |
| 6 | `terminal.implementation`, chooser dialog, `Command::NewTerminalIn`, both extensions answering it | ext-api, ui, terminal, terminal-native | E2E: both enabled → chooser appears, *remember* sets the setting |
| 7 | verify, log, vault, catalogue | | |

## Risks
| risk | mitigation |
|---|---|
| `stream-json` shapes differ between CLI versions | codec keeps unknown kinds (logged, skipped); tests pin the mock to what 2.1.278 emits, recorded in the crate note |
| A headless `claude` waits on a permission prompt it cannot show | `plan` mode by default; `acceptEdits` is a setting the user turns on; a stuck process is killed at the turn timeout (10 min) with the reason in the transcript |
| Server sessions and the local agent drift apart | one `Item` model, one transcript writer (`transcript.rs`) used by both; the panel differs only in where `send` goes |
| A Rust terminal that is worse than xterm.js gets used by mistake | opt-in tier, the chooser names both, xterm.js stays the default until parity |
| Keyboard encoding gaps (dead keys, IME, Ctrl+Shift combos) | printable input through an `<input>` field that mirrors composition; the rest by explicit tables, tested |
