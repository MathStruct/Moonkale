---
title: "Milestone 11 — Remote: the plan"
description: Work on another machine from the desktop app the way Zed does it — Moonkale starts its own server on the remote host through the system ssh and uses it as a source — plus the desktop app as a client of any Moonkale server, and the hardening an exposed server needs. Record in Milestone 11 - Implementation Log.
tags: [milestone, planning]
---
> [!success] Done 2026-09-21 — see [[Milestone 11 - Implementation Log]] (deviations: ControlMaster used for the upload channel; no checksum yet; *Connect to Server…* dialog deferred).

**Goal** (from [[Remote and Server Modes]], mode 1 first): open a folder on another machine from the desktop app, with the folder, index, LSP, git and terminal running *there*, credentials staying in `ssh`, and LLM keys staying *here*. Daniel works through code-server on another PC today; after this milestone he opens that PC's folders in Moonkale directly. Record: [[Milestone 11 - Implementation Log]].

## Starting point (after Milestone 10)
- The web server exists with token auth, jail, audit, relays for terminal/LSP/git/Typst ([[Milestone 7 - Implementation Log]]); the web client talks to it through `api`'s server-function clients (`RemoteSource`, `RemoteTerminal`, `RemoteLsp`, `git_run`, presence).
- Those clients compile natively too: the desktop build has `dioxus/fullstack` **without** the `server` feature, so a server function called from the desktop is an HTTP call to `dioxus::fullstack::get_server_url()` — which nothing sets yet. `set_server_url` and `set_request_headers` (for a bearer) exist in dioxus-fullstack 0.7.10.
- The desktop already joins a hub for presence (`MOONKALE_HUB`); it opens only local sources.
- `ssh` and `sshd` are on the machine; the `moonkale-terminal-pty` crate runs PTYs, so an `ssh` prompt can appear in the terminal panel.

## Scope: what "done" means
1. **Desktop as a client of a Moonkale server** — `File → Connect to Server…` (URL + token) or `MOONKALE_REMOTE=http://host:port` + `MOONKALE_TOKEN`: the desktop's `WorkspaceConfig` switches to the client wiring the web build uses (sources, terminal, LSP, git, Typst, presence through the server) while the **LLM provider stays local** with local keys. The server's folders open like local ones; the source row shows the remote icon. Disconnect returns to local mode.
2. **Remote folder over SSH** — `File → Open Remote Folder…` (host from `~/.ssh/config` or typed, path): a `remote` crate drives the **system `ssh`** in a PTY (prompts for passphrases/passwords/host keys appear in the terminal panel, nothing is stored), checks for the server binary under `~/.local/share/moonkale/server/<version>/` on the host, uploads the matching release if missing (checksum verified), starts it bound to `127.0.0.1:<port>` with a **per-session token passed over stdin**, forwards the port (`-L 127.0.0.1:<local>:127.0.0.1:<remote>`), and then does step 1 against `http://127.0.0.1:<local>`. Closing the source or quitting ends the session; the remote server dies with it.
3. **Server hardening that this exposes** — `MOONKALE_TERMINAL=0` (no shell on servers meant for editing), an explicit Origin check on websocket upgrades, refusal to bind a non-loopback address without TLS unless `MOONKALE_INSECURE_HTTP=1`, and built-in TLS (`MOONKALE_TLS_CERT` / `MOONKALE_TLS_KEY`, rustls) so a reverse proxy is optional.
4. **A standalone server binary** — `moonkale-server` (the `web` crate's server half) with `--port`, `--bind`, `--root`, `--token-stdin`, `--version`, buildable for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`; this is what gets uploaded.
5. Verify, log, vault.

**Deferred**: accounts and roles (mode 3), a Windows remote (no `ssh` PTY story yet), the remote *server* launching LSPs that are not installed there, syncing settings between machines (that is [[Projects and Sources]]).

## Architecture decisions
- **The system `ssh`, never a Rust SSH client**: `~/.ssh/config`, agents, `known_hosts`, ProxyJump and every prompt behave exactly as in a terminal, and Moonkale holds no credential — see the security section of [[Remote and Server Modes]].
- **One session = one `ssh` process** with the port forward and the remote command; a second `ssh` for the upload only when needed. No ControlMaster (it would keep sockets around after Moonkale exits).
- **The desktop stays the client of its own local sources or of one server at a time** in this milestone; mixing local and remote sources in one workspace is the projects work.
- **Tokens are generated per session** (32 random bytes, hex), sent to the remote process on stdin, kept in memory on the desktop; the bearer header is set through `set_request_headers`.
- **LLM keys never travel**: the agent runs locally against the desktop's provider; its tools reach the remote sources through the same `Workspace` they always did.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | `api::client` — the client `WorkspaceConfig` half shared by web and desktop; `RemoteProvider` no longer wasm-only (unused in remote mode but compiled); desktop `--remote <url>` / `MOONKALE_REMOTE` + token → `set_server_url` + bearer; source icon `folder-remote` | api, web, desktop, ui | native test: desktop client config against the E2E server (`cargo test -p desktop --test remote -- --ignored` with `MOONKALE_REMOTE`) |
| 2 | `moonkale-server` binary flags (`--port --bind --root --token-stdin --version`), `MOONKALE_TERMINAL=0`, Origin check | web, api | `auth.mjs` extended (token over stdin, terminal off, cross-origin upgrade refused) |
| 3 | `remote` crate: `SshSession::open(host, path)` — probe, upload, start, forward, token; PTY prompts in the terminal panel; teardown | remote, desktop, terminal | native test with a **fake `ssh`** on `PATH` (a script that runs the local server and forwards nothing); a real `ssh localhost` run when keys allow |
| 4 | UI: File → Open Remote Folder… (hosts from `~/.ssh/config`), status ("connecting… / uploading server / connected"), Close Source ends the session; palette commands | ui, desktop | E2E on the web client is not possible; desktop verified by the native test + Daniel's look |
| 5 | TLS built in, insecure-bind refusal | web, api | native: bind refused without TLS; `auth.mjs` over https with a self-signed cert |
| 6 | verify, log, vault | | |

## Risks
| risk | mitigation |
|---|---|
| `ssh` prompts (passphrase, host key) block a hidden PTY | the session's PTY *is* a terminal panel tab from the first byte, so prompts are visible and typed by the user; a timeout aborts with a message |
| Port-forward races (remote port taken) | pick a random high port, retry on `bind` failure reported by the server on stdout before it accepts the token |
| Uploading the wrong binary (target triple) | the desktop asks `uname -m` first and refuses without a matching release asset; a version mismatch is refused too |
| The desktop's server-function client is shared global state (`set_server_url`) | one server at a time this milestone; switching disconnects first |
