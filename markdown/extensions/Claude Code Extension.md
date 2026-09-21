---
title: "Claude Code extension — plan"
description: How Moonkale becomes Claude Code's IDE and Claude Code becomes one of Moonkale's agents — on a Claude subscription, without an API key — in three levels (terminal, IDE bridge, agent provider), with Moonkale's policy gate in front of Claude Code's own permissions.
tags: [extensions, agents, claude-code, planning]
---
From [[Prompt14]] (2026-09-19): an extension for Claude Code itself, usable without a Claude API key. **It is possible**: the `claude` CLI (2.1.278 is installed here) authenticates with the subscription login stored by `claude login` in `~/.claude/`; anything that *spawns the CLI* inherits that, and Moonkale never sees a credential. What is *not* possible without a key is calling the Anthropic API directly — so this extension never does; the `anthropic` provider in `moonkale-llm` stays for people who have a key.

## Three levels, each useful on its own

### Level 1 — Claude Code in the terminal (exists, zero work)
The terminal panel runs a PTY on desktop and on the server; `claude` runs in it like in any terminal, with its own TUI, permissions and `claude login`. What is missing is only that Claude Code does not know it is inside Moonkale: it cannot see the selection, open a file in a tab, or show a diff. That is Level 2.

### Level 2 — Moonkale as Claude Code's IDE (the IDE bridge)
Claude Code discovers an IDE through **lock files** in `~/.claude/ide/<port>.lock` — on this machine there are four from code-server, each `{ "pid", "workspaceFolders": [...], "ideName", "transport": "ws", "runningInWindows", "authToken" }` — and connects to a **WebSocket MCP server** on that port (header `x-claude-code-ide-authorization: <authToken>`); `claude --ide` or `/ide` picks it when exactly one matches the working directory. The IDE side is an MCP server whose tools the CLI calls and whose notifications it listens to. Moonkale implements that server natively (desktop: in-process with `tokio-tungstenite`, the crate the presence client already uses; web: on the server for its `MOONKALE_ROOT`):

| direction | message | Moonkale does |
|---|---|---|
| CLI → IDE tool | `openFile { filePath, preview, startText, endText, selectToEndOfLine }` | open the tab, select the range |
| | `openDiff { old_file_path, new_file_path, new_file_contents, tab_name }` | a diff tab (the git extension's diff panel) with **Accept / Reject**; the reply (`FILE_SAVED` / `DIFF_REJECTED`) is what the CLI waits for |
| | `getCurrentSelection`, `getLatestSelection`, `getOpenEditors`, `getWorkspaceFolders` | from `Workspace` (active document, cursor/selection, open tabs, open folder sources) |
| | `getDiagnostics { uri? }` | the LSP diagnostics the code editor already holds |
| | `checkDocumentDirty`, `saveDocument`, `close_tab`, `closeAllDiffTabs` | document state / save path / tab ops |
| | `executeCode` (notebooks) | not supported — returns an error; notebooks are not an editor yet |
| IDE → CLI notification | `selection_changed { text, filePath, fileUrl, selection }` | sent on cursor/selection change (throttled like presence) |
| | `at_mentioned { filePath, lineStart, lineEnd }` | *Send to Claude Code* on the editor's context menu — pastes `@file:line-line` into the running CLI |
| | `ide_connected { pid }` | on connect |

Exact names and payloads are to be **verified against the open-source reimplementation** (`coder/claudecode.nvim`, whose `PROTOCOL.md` documents what the VS Code extension does) before coding; the table is from that protocol as known on 2026-09-19. Lock files are per Moonkale window, written on start and removed on exit (stale ones from crashes are ignored by pid).

The result: a `claude` running in Moonkale's terminal panel — started with `--ide` or through a *Claude Code: Start here* command that opens a terminal with the right cwd — sees what you have selected, opens files in your tabs, and proposes every edit as a diff you accept in Moonkale. This is what the VS Code extension gives, with no API key, and it is the level with the best value per line of code.

### Level 3 — Claude Code as a Moonkale agent provider *(built in Milestone 12 — see [[Milestone 12 - Implementation Log]]; the provider lives in `moonkale-llm` behind the `claude-code` feature, with `permission_mode` + `allowed_tools` instead of the permission tool, and `--resume` per folder)*
The agent panel talks to a `Provider` (`anthropic`, `openai`, `ollama`, `mock`, `remote`). A **`claude-code` provider** spawns the CLI headless:

```text
claude -p --output-format stream-json --input-format stream-json --include-partial-messages \
       --session-id <uuid> [--resume <uuid>] \
       --mcp-config <moonkale-mcp.json> --permission-prompt-tool mcp__moonkale__approve \
       --add-dir <open folder sources> [--permission-mode plan|default] [--restricted]
```
- **Streaming both ways**: `stream-json` on stdin/stdout — user turns go in as JSON lines, assistant deltas, tool uses and results come out; the provider maps them onto Moonkale's `Event` stream so the agent panel, transcripts-as-nodes and the audit log work unchanged. `--session-id`/`--resume` give continuity across panel reopen; the CLI's own JSONL under `~/.claude/projects/<slug>/` is the transcript of record (and an index source later — sessions as nodes).
- **Moonkale's policy gate in front of Claude Code's permissions**: `--permission-prompt-tool` routes every permission question the CLI would ask to an MCP tool that Moonkale provides — the same approval prompt the agent panel already shows (`policy.rs`: reads allowed, writes ask, destructive always ask) — and Moonkale answers allow/deny. Claude Code's own rules (`.claude/settings.json`, `allowedTools`) still apply underneath; the two compose, they do not replace each other. On the server the jail (`MOONKALE_ROOT`) bounds `--add-dir`.
- **Moonkale's tools for Claude Code**: `--mcp-config` points at Moonkale's MCP server (`/mcp`, Milestone 5: `workspace_list_sources`, `graph_query`, `graph_fetch`, `index_search`, `source_text_query`, read-only) plus, on desktop, an in-process MCP endpoint with the `editor_*` tools and `approve`. So Claude Code can query the graph, the databases and the hybrid search it could not reach on its own — the argument for using it *inside* Moonkale rather than next to it.
- `--bare` is **never** used (it disables the OAuth login, i.e. the subscription). `--restricted` is a checkbox in the provider's settings for a read-only assistant.

## Shape
`packages/extensions/claude-code/` — a static Rust extension in the `opt_in` tier (`ui::default_extensions()`), enabled in *Settings → Extensions*:
- `ide.rs` — lock file + WebSocket MCP server (desktop in-process; `api` route on the server, gated by `MOONKALE_TOKEN` like everything else); contributes the `selection_changed` publisher and the *Send to Claude Code* command.
- `provider.rs` — the `claude-code` `Provider` (spawns the CLI; `stream-json` codec; permission tool; MCP config file written to a temp path with the server's URL and token).
- `commands.rs` — `claude.start` (terminal with `--ide`), `claude.continue` (`--continue`), `claude.at_mention`, `claude.open_session` (the CLI's JSONL as a page).
- `claude-code.md` (crate note), settings: `path` to the binary, `permission_mode`, `restricted`, `model` (`--model`, subscription-permitting), `add_dirs`.
- Platforms: **desktop and server** (need the binary; Node underneath — a dev-tool dependency, not shipped, [[JavaScript Inventory]]); the **web client** gets Level 3 through the server (`RemoteProvider`, as for the other providers) and Level 2 through the server's bridge; **not on the phone** (no CLI) unless it talks to a desktop hub.

## Steps
1. `stream-json` codec + spawn + provider; mock `claude` script for tests (a shell script that speaks the protocol) — native tests for the codec, E2E `claude-code.mjs` with the mock on the server.
2. Permission tool via MCP → Moonkale's gate; audit lines.
3. IDE bridge: lock file, WS server, `getCurrentSelection`/`openFile`/`getOpenEditors`/`getWorkspaceFolders` first, then `openDiff` with the diff panel, `getDiagnostics`, `selection_changed`; verified against the real CLI on this machine (`claude --ide` in the terminal panel).
4. `at_mentioned`, commands, sessions as pages.
5. Docs: crate note, [[LLM and RAG]] "External agents" (this decides the open question there: MCP, both directions), Problem Log for what the protocol turned out to be.

## Risks
| risk | mitigation |
|---|---|
| The IDE protocol is undocumented by Anthropic and changes with the CLI | pin the CLI version tested; the nvim reimplementation tracks changes; the bridge degrades to Level 1 when the handshake fails |
| `stream-json` shapes change between CLI versions | codec tolerant of unknown event kinds (log-and-skip, as the CodeMirror bridge does — P-077); tests against the pinned version |
| Two permission systems confuse the user | one prompt (Moonkale's); Claude Code's own settings shown read-only in the provider's settings tab |
| Subscription rate limits | surfaced as the CLI reports them; nothing to hide |

## Not in scope
Calling the Anthropic API directly (needs a key — the existing `anthropic` provider), embedding Claude Code's TUI in a panel (the terminal already is that), running the CLI on the phone.
