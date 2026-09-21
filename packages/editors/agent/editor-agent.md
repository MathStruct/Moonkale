---
title: "editor-agent — implementation notes"
tags: [crate-notes, milestone-4]
---
Notes for `moonkale-editor-agent` (Milestone 4). Design: [[LLM and RAG]].

- `AgentExtension` contributes the **Agent** panel (home `right`; the default layout now has a right tile).
- `panel.rs` — `Chat` state in ROOT signals (items, the `Agent`, provider label, busy, pending approval, cited paths, an **audit mirror**). The provider comes from `WorkspaceConfig::llm` (desktop: in-process from env; web: `api::RemoteProvider`). Each send rebuilds the system prompt from the workspace (open sources with ids, the active document) and runs `Agent::send`; events become chat items: streamed assistant text, tool cards (name · class · decision · outcome · summary), errors. `Ask` decisions render an approval box with Allow/Deny. **Save** writes the transcript to `.moonkale/chats/<slug>-<id>.md` through `Workspace::create_text` (indexed, wiki-links to cited files under *Cited*), then opens it. **Activity** shows the audit log.
- `host.rs` — `WorkspaceHost: ToolHost`: executes the six tools against `Workspace` (sources by id, `Query::*`, `fetch_text`, `Query::Text{search}` on the index, `reveal` for `editor.open` with a line); tracks cited paths; `approve` hands a oneshot to the panel.
- `transcript.rs` — markdown rendering of a conversation (You / Agent sections, tool results as fenced blocks, Cited, Tool calls table).
- The agent is mutably borrowed for the whole exchange (`busy` keeps everything else off it; P-064).

E2E: `packages/web/tests/e2e/agent.mjs` (mock provider through the relay: echo, a read-only tool round, an `Ask` denied, Activity, Save → file opens).

## Milestone 5
- The provider is built from `ws.settings.llm` and rebuilt when those settings change (conversation kept); the policy follows `ws.settings.policy` (`allow_writes` → mutating tools run without asking; destructive ones still ask).
- Write tools in `host.rs`: `editor.replace` edits the open `Document` (exact, unique `old` → `new`; the user saves), `file.create` uses `Workspace::create_text`, `terminal.run` spawns a session through `spawn_terminal`, writes `cmd; exit $?`, collects the output until the stream closes (ANSI stripped, 200 KB cap).
- Approval box: a red/green diff for `editor.replace`, a `$ command` card for `terminal.run`.

## Milestone 6
- `wasm_tools(ws)`: every `llm_tool` command of an enabled wasm extension becomes a `ToolDef` (`<ext>.<name>`, its `input_schema`); `wasm_owner` maps a tool name back to the extension and `host.rs` dispatches through `Workspace::run_wasm_command` (granted permissions from settings). Policy: unknown/third-party tools are `Mutating` → *Ask* unless `allow_writes`.
- Permissions gate the built-in write tools too: an agent extension without `write-files` granted denies `editor.replace` / `file.create`, without `run-commands` denies `terminal.run` (Settings → Extensions → Agent).

## Milestone 12
- `server_panel.rs`: `ServerAgentPanel` — used by the extension's `render` when `ws.agent_sessions()` is `Some` (the sources are a server's and `agent.on_server` is set, or the platform has no local provider). Sends through `AgentSessions::send`, polls `events(since)` every 700 ms while a turn runs (re-reading the last item, which may still be streaming; a final full fetch at the end), lists the folder's sessions (a running one is opened on arrival), answers approvals with `approve`. Root-scope signals, so the state survives panel remounts. `sleep_ms` (gloo-timers on wasm, tokio natively).
- `panel.rs`: the local agent's `cwd` is the first open folder (`Request.cwd`, what the `claude-code` provider runs in).
