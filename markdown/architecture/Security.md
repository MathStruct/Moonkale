---
title: "Security — the trust model in one page"
description: Who and what Moonkale trusts on each platform — the server token, secrets, the folder jail, the agent's policy gate, extension permissions, SSH sessions — what is enforced where today, and the open findings.
tags: [architecture, security]
---
The pieces are described where they were built ([[Remote and Server Modes]], [[LLM and RAG]], [[Extension System]], [[Agent Sessions and Profiles]]); this page puts them side by side. The open findings of the external audit are in [[Audit 2026-09-23]].

> [!warning] State on 2026-10-01
> Moonkale is a prototype. Eleven of the twenty audit issues are security findings; the structural part of #1, #4 and #8 is fixed on the `refactor` branch (Milestone 18 phase 4.5: the server and the user decide, not a client or a folder), the rest are open. Run a server only on loopback, behind SSH, or on a private network (WireGuard, Tailscale) until they are fixed.

## Boundaries

| boundary | what is trusted | enforced by | where in the code |
|---|---|---|---|
| **Server ↔ browser/desktop client** | whoever holds `MOONKALE_TOKEN` gets every file under `MOONKALE_ROOT` and, unless `MOONKALE_TERMINAL=0`, a shell as the server's user | cookie (HttpOnly, SameSite=Strict) or `Authorization: Bearer`; `/login` rate-limited; non-loopback bind refused without a token (`guard_bind`); TLS built in (`MOONKALE_TLS_CERT`/`_KEY`), plain HTTP off loopback refused unless `MOONKALE_INSECURE_HTTP=1`; Origin check on websocket upgrades; COOP/COEP | `server/src/auth.rs`, `web/src/main.rs` |
| **External agents (MCP)** | a separate bearer, `MOONKALE_MCP_TOKEN` | `server/src/mcp.rs` | — |
| **Paths** | everything below `MOONKALE_ROOT` (server) or the opened folder (desktop) | canonicalisation in `moonkale_server::open_any`, relative keys in `project-fs` | symlinks and Windows paths escape it (#5) |
| **Secrets** | names in settings, values never | resolved from `MOONKALE_SECRET_<NAME>`, the provider's usual variable, or `<config>/moonkale/secrets.json` (mode 600); web clients never see a value | `llm/src/secrets.rs`; an OS keychain (`keyring`) is planned, not built |
| **The agent** | a model that may propose any tool call | the policy gate: reads allowed, writes ask, destructive always ask; SQL statements classified; every call in the audit log | `llm/src/policy.rs`, `core/src/source/risk.rs` (`Source::classify`, overridable per source since Milestone 18); Cypher is a keyword scan, and LadybugDB runs any statement on its read-only database (#9) |
| **Extensions (wasm)** | a module with the permissions the user granted | JSON ABI v1: every host call checked against the grant; grants come from the user scope only (a folder cannot grant, Milestone 18 phase 4.5); an id belongs to the file that loaded it first, so a folder's module cannot squat a user-installed module's id and grants | `ext-host`; modules still have no fuel or memory limits (rest of #4) |
| **Extensions (static)** | compiled in; trusted like the rest of the binary | the tier (`core`/`optional`/`opt_in`) only decides what is on; the distribution's Cargo features decide what is in the binary | `moonkale_distribution::default_extensions()` |
| **A folder's own settings** | `.moonkale/settings.json` of a folder you open — **data, not authority** (Milestone 18 phase 4.5) | `SettingsFile::without_authority` before the merge: no language model or agent, no auto-approved writes, no embeddings switched on, no grants, no shell, no SSH hosts; it may switch things off and deny tools (added to the user's denials). Settings shows what was ignored | `ext-api/src/settings.rs`; test `a_folder_has_no_authority` |
| **SSH remote folders** | the system `ssh` and the user's keys; Moonkale never sees a password | the server binary uploaded per version, a per-session token typed over stdin with echo off, loopback-only port | `remote/src/session.rs`; temp names and the control socket (#18); the remote server is not checksummed yet |

## The LLM relay (server)
Since Milestone 18 phase 4.5 the server decides what a client's provider settings may make it do (`moonkale_server::llm::server_side`, audit #1): the client picks the kind, the model and the secret's *name*; a secret goes only to the provider's built-in endpoint or one the operator listed in `MOONKALE_LLM_ENDPOINTS` (comma-separated URL prefixes), and `claude-code` runs `MOONKALE_CLAUDE_BIN` or `claude` on the server's `PATH`, never a path from the client. A refusal reaches the client as an error, not a silent fallback.

## Rules that follow from the audit
1. **The server decides.** Anything that turns into a process, a network request with a secret, or a permission is resolved from the server's own settings, never from a value the client sends (#1, #4, #8). This is the main design input to phase 4 of [[Milestone 18 - Library Refactor]].
2. **The workspace scope is data, not authority.** A folder may choose a theme, a layout or an editor; it may not name a command to run, grant a permission or point a secret at an endpoint (#8).
3. **Read-only means read-only for the engine too.** A driver opened "read-only" must also be unable to read outside its file (#2: DuckDB's `read_csv`, `glob`).
4. **Every buffer has a bound** (#14), every path goes through one jail function (#5), every link scheme through one allow-list (#10).

## Not built
Accounts and roles for a shared server (design in [[Remote and Server Modes]]); a Content-Security-Policy; signed releases and a checksum for the uploaded remote server; an OS keychain.
