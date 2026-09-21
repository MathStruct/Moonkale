---
title: "Remote and server modes — SSH, a Moonkale server, collaboration"
description: Working on another machine than the one Moonkale runs on - three modes (remote folder over SSH like Zed, a Moonkale server used from a browser or the desktop app like code-server, a shared server for several people), what exists today, and the security considerations of each - credentials, transport, what a token buys, isolation, the terminal.
tags: [architecture, remote, security, collaboration, design]
---
From Daniel's question (2026-09-20): he works in a browser against code-server on another PC and asks whether Moonkale can do the same — open a remote folder and terminal over SSH like Zed, or run a Moonkale server reached from the desktop app or the web — and what the security considerations are, given an SSH key in `~/.ssh` and/or a password for the remote machine. Related: [[Projects and Sources]] (remote folder as a source kind), [[Publishing Sources]] (reader mode), [[Collaboration]], [[Milestone 7 - Implementation Log]] (token auth).

## What exists today

| piece | state |
|---|---|
| **Moonkale server + web client** (the code-server model) | ✅ `packages/web`: the client is served by the same process that holds the sources; terminal (PTY), LSP, git, Typst, the agent and wasm extensions run **on the server**; `MOONKALE_ROOT` jails every path; `MOONKALE_TOKEN` gates every request (HttpOnly, SameSite=Strict cookie from `/login`, or `Authorization: Bearer`; `Secure` when behind an HTTPS proxy); `/login` rate-limited (10/min per address); every `/api/*` and `/mcp` request writes an audit line; the server **refuses to bind a non-loopback address without a token** (`guard_bind`); COOP/COEP headers for the browser wasm runtime |
| TLS | ✅ built in since Milestone 11: `MOONKALE_TLS_CERT` + `MOONKALE_TLS_KEY` (PEM, rustls); off loopback the server refuses plain HTTP unless `MOONKALE_INSECURE_HTTP=1` says a reverse proxy terminates TLS |
| user accounts / roles | ❌ one token = one identity; presence names are self-declared (`Settings → You`) |
| **Desktop app as a client of a remote server** | ✅ Milestone 11: `MOONKALE_REMOTE=http://host:port` + `MOONKALE_TOKEN` (or a session's forwarded port) makes the desktop a client of that server for sources, terminal, LSP, git, Typst and wasm extensions (`api::client`); the LLM provider stays local |
| **Remote folder over SSH** (Zed model) | ✅ Milestone 11: `File → Open Remote Folder…` (`moonkale-remote`) — the system `ssh` in a terminal tab, the server uploaded once per version into `~/.local/share/moonkale/server/<version>/`, started on loopback with a per-session token over stdin, port forwarded, closed with the source. Details in [[Milestone 11 - Implementation Log]] |
| Standalone server binary + hardening | ✅ `moonkale-server --port --bind --root --token-stdin --version`; `MOONKALE_TERMINAL=0` switches the terminal off; cross-origin websocket upgrades refused |
| **Collaboration** | presence only (who is here, which file, which line — [[Milestone 8 - Implementation Log]], [[Milestone 9 - Implementation Log]]); no shared editing (CRDT deferred, [[Collaboration]]) |

**Usable right now** (Milestone 11): `File → Open Remote Folder…` in the desktop app, or `moonkale --ssh "[VAR=v] [ssh options] host:/path"` — the host field takes everything you would type after `ssh` (`-p 443`, `-i key`, `SSH_AUTH_SOCK=0`, aliases). **By hand, with nothing but ssh**: on the remote machine run the server on loopback (`MOONKALE_ROOT=/home/me/Code MOONKALE_TOKEN=… ./moonkale-server --port 8080` — or `dx serve --port 8080` in `packages/web`), then from the laptop `ssh -L 8080:127.0.0.1:8080 host` and open `http://127.0.0.1:8080`. The only credential in play is the SSH key or password already in use; the token is a second lock on the tunnel's local end. This is exactly the code-server situation, with the same trust model.

## The three modes

### 1. Remote folder and terminal over SSH (Zed's model) — recommended for one person, two machines
Moonkale on the laptop starts (or connects to) **its own server on the remote host through SSH** and treats that connection as a source: the folder, the index, LSP, git and the terminal run *there*, the editor and the UI run *here*. The desktop app speaks the same server-function protocol it would speak to any Moonkale server, over a port forwarded inside the SSH session.

How it would work:
1. **Transport**: the system `ssh` binary, never a re-implementation. `ssh -o ExitOnForwardFailure=yes -L <local>:127.0.0.1:<remote> host moonkale-server --stdio-or-port …`. This inherits `~/.ssh/config` (aliases, ProxyJump, IdentityFile), the agent, `known_hosts`, and password prompts. Moonkale runs `ssh` in a PTY it owns — the terminal panel already does that — so a passphrase or password prompt appears in Moonkale and is typed by the user; Moonkale never reads, stores or forwards the secret.
2. **The remote binary**: Moonkale checks for `~/.local/share/moonkale/server/<version>/moonkale-server` on the host, and if missing offers to upload the matching release (checksum verified against the version the desktop was built with — never `curl | sh`). Zed does the same. The binary runs as the SSH user, binds `127.0.0.1` only, with `MOONKALE_ROOT` = the chosen folder, and dies with the SSH session.
3. **Identity inside the session**: the forwarded port is reachable only from the laptop's own loopback, but any local process could reach it — so the server still requires a token: Moonkale generates one per session (random, 32 bytes), passes it to the remote through the SSH channel's environment or stdin (never a command-line argument, which `ps` shows), and uses it as the bearer. Nothing is written to disk on either side.
4. **What runs where**: sources, index, LSP, git, terminal PTY, wasm extensions (wasmtime) — remote. The **LLM provider stays local**: the agent's model calls go from the laptop with the laptop's keys, and its tools reach the remote sources through the source proxy, so API keys never leave the machine that owns them. (A remote-side provider would be a per-source choice, off by default.)
5. **The terminal** is a PTY on the remote host as the SSH user — the same shell you would get with plain `ssh host`, inside Moonkale's terminal panel; the trace-to-graph feature works unchanged.

Security considerations, specific to this mode:
- **Credentials**: keys stay in `~/.ssh` and the agent; a password is typed into the `ssh` prompt in Moonkale's PTY and goes straight to `ssh`. Moonkale adds no credential store for SSH and must not (a stored password in a settings file would be the weakest link on the machine). Recommend keys + agent; passwords work but are typed every time.
- **Host keys**: first connect to an unknown host shows `ssh`'s own fingerprint prompt in the PTY; Moonkale never passes `StrictHostKeyChecking=no`. A changed host key is a hard stop, as with plain `ssh`.
- **Agent forwarding is not needed** and must not be enabled by default (a compromised remote could use the agent).
- **The remote binary is the trust boundary**: it has the SSH user's rights on that host. Uploading it means running code there — the same trust as `pip install` on that machine; verify the checksum, keep the version pinned to the desktop's, keep it under the user's own home.
- **Local exposure**: the forwarded port on the laptop is loopback-only *and* token-gated; other users on a shared laptop cannot use the session.
- **Blast radius**: if the laptop is compromised, the attacker has what the SSH user has on the remote — identical to plain SSH; Moonkale does not widen it. If the remote is compromised, the laptop's secrets (LLM keys, `secrets.json`) are not on it; only the session token, which dies with the session.
- **Logging**: the remote server's audit log records the session's requests; Moonkale's history (`.moonkale/history.jsonl`) lives with the folder, on the remote.

### 2. A Moonkale server on the network (code-server's model)
The server runs permanently on a host and is reached over the network from a browser (today) or the desktop app (soon). Same process as mode 1, but exposed rather than tunnelled — which is where every consideration comes from:

- **TLS is mandatory for anything but loopback**: the token travels in a cookie/bearer header; without TLS it is readable on the wire. Today: a reverse proxy (Caddy does automatic certificates; nginx with certbot) in front, with `X-Forwarded-Proto: https` so the cookie gets `Secure`. Planned: built-in `MOONKALE_TLS_CERT` / `MOONKALE_TLS_KEY` (rustls) and a refusal to bind a non-loopback address without either TLS or an explicit `MOONKALE_INSECURE_HTTP=1` — the token check alone is not enough.
- **The token is a shell**: whoever has `MOONKALE_TOKEN` gets the terminal, i.e. a shell as the server's Unix user, plus every file under `MOONKALE_ROOT`. Treat it like an SSH private key: ≥ 32 random bytes, never in a URL, never in a shared settings file, rotated if leaked (restart with a new value — sessions are cookies of the token itself, so rotation logs everyone out). A **`MOONKALE_TERMINAL=0`** switch (planned) turns the shell off for servers that are only meant for editing.
- **Network placement beats every other control**: keep the server on a private network — WireGuard or Tailscale between your machines — or behind the SSH tunnel of mode 1. Exposing it to the Internet is possible with TLS + token but is the least defensible option because rate-limited login is the only brute-force protection and there is no lockout, no 2FA, no accounts.
- **Isolation**: one server = one Unix user. Run it under a dedicated user with only the folders it should see (`MOONKALE_ROOT`), not your login user; systemd `ProtectSystem=strict`, `ProtectHome=tmpfs` + `BindPaths` for the root, `NoNewPrivileges`, a memory limit, and a private `MOONKALE_CONFIG_DIR` so extension modules and secrets are separate. Several people → several servers (or mode 3 with accounts).
- **Secrets on the server**: LLM keys live in `<config>/secrets.json` (0600) or the environment, never in `settings.json`; the agent's tool calls run under the policy gate with the audit log; wasm extensions run in wasmtime with per-call permissions. The MCP endpoint has its own token (`MOONKALE_MCP_TOKEN`) so an external agent's credential is not the human's.
- **Browser hardening already in place**: SameSite=Strict + HttpOnly cookie (no CSRF from other sites, no script access), COOP/COEP (isolation), Origin checks on the websocket to add (planned: reject cross-origin `Upgrade`s explicitly).
- **What the desktop-as-client adds**: the desktop app connects with the bearer token stored in the OS keychain (`SecretRef`, as for LLM keys), pins the server's certificate on first use (TOFU, shown to the user), and then offers the same panels over the remote sources; its terminal panel is the server's PTY. The local LLM-keys rule of mode 1 applies.

### 3. A shared server for several people (what Daniel first imagined)
Several people connect to one server, see each other (presence exists), and eventually edit together. Everything in mode 2 applies, plus the parts that only matter with more than one identity:

- **Accounts**: a `users.toml` (name, token hash — argon2 — role) under the config dir, or OIDC later for a team that already has an identity provider; `/login` takes name + token; presence, history (`actor`), annotations ([[Annotations]]) and the audit log carry the *authenticated* name, not a self-declared one.
- **Roles**: `reader` (sources, search, graph, history), `editor` (+ save, file ops, agent under policy), `admin` (+ terminal, settings, extensions). The terminal is admin-only on a shared server — a shell as the server user is not something to hand to a group.
- **Per-user policy and secrets**: each account's LLM provider and policy (or none — a shared provider with a shared budget is an admin decision); each account's approval prompts are its own.
- **Shared editing**: presence today; next the entity log relayed through the hub (every save is an event other windows apply — last writer wins, with the conflict surfaced), then CRDT text merge for simultaneous typing ([[Collaboration]]). None of this changes the security model: the hub relays what the roles allow.
- **Rooms are folders**: a user sees the folders their role grants; `MOONKALE_ROOT` stays the outer jail.

## Recommendation
- **Now (built in Milestone 11)**: mode 1 — `File → Open Remote Folder…`; the remote-folder session over the system `ssh`, the server uploaded once per version, a per-session token, LLM keys local. Mode 2 with the desktop app or a browser behind SSH port forwarding / WireGuard / built-in TLS also works.
- **Then**: accounts + roles — what makes mode 3 exposable. Collaboration proper (shared history, CRDT) after that.

## Decided in Milestone 11
The desktop **uploads** the server binary (Zed's way; no install step on the host), over the same multiplexed `ssh` connection; the token travels over **stdin** (`--token-stdin`), typed into the PTY with echo off — `SendEnv` needs `AcceptEnv` on the host's `sshd`, which most hosts do not allow. Still open: the account store format (`users.toml` vs OIDC first); a checksum on the uploaded binary once releases are signed.
