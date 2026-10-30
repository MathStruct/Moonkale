---
title: "editor-agent — implementation notes"
tags: [crate-notes, milestone-4]
---
Notes for `moonkale-editor-agent` (Milestone 4). Design: [[LLM and RAG]].

- `AgentExtension` contributes the **Agent** panel (home `right`; the default layout now has a right tile).
- `panel.rs` — `Chat` state in ROOT signals (items, the `Agent`, provider label, busy, pending approval, cited paths, an **audit mirror**). The provider comes from `WorkspaceConfig::llm` (desktop: in-process from env; web: `moonkale_server::RemoteProvider`). Each send rebuilds the system prompt from the workspace (open sources with ids, the active document) and runs `Agent::send`; events become chat items: streamed assistant text, tool cards (name · class · decision · outcome · summary), errors. `Ask` decisions render Allow/Deny in the session's Proposals inbox. **Save** writes the transcript to `.moonkale/chats/<slug>-<id>.md` through `Workspace::create_text` (indexed, wiki-links to cited files under *Cited*), then opens it. **Activity** shows the audit log.
- `host.rs` — `WorkspaceHost: ToolHost`: executes the six tools against `Workspace` (sources by id, `Query::*`, `fetch_text`, `Query::Text{search}` on the index, `reveal` for `editor.open` with a line); tracks cited paths; `approve` hands a oneshot to the panel.
- `transcript.rs` — markdown rendering of a conversation (You / Agent sections, tool results as fenced blocks, Cited, Tool calls table).
- The agent is mutably borrowed for the whole exchange (`busy` keeps everything else off it; P-064).

E2E: `packages/web/tests/e2e/agent.mjs` (mock provider through the relay: echo, a read-only tool round, an `Ask` denied, Activity, Save → file opens).

## Milestone 5
- The provider is built from `ws.settings.llm` and rebuilt when those settings change (conversation kept); the policy follows `ws.settings.policy` (`allow_writes` permits ordinary mutating tools; text tools still queue proposals and file creation still asks; destructive tools still ask).
- Write tools in `host.rs`: `editor.replace` queues an exact, unique, versioned `old` → `new` proposal; `editor.propose` queues a multi-file changeset, `file.create` uses `Workspace::create_text`, `terminal.run` spawns a session through `spawn_terminal`, writes `cmd; exit $?`, collects the output until the stream closes (ANSI stripped, 200 KB cap).
- Approval inbox: source/file-labelled original/proposed text, selected-file approval/rejection, stale three-way review and re-review; a `$ command` card for `terminal.run`.

## Milestone 6
- `wasm_tools(ws)`: every `llm_tool` command of an enabled wasm extension becomes a `ToolDef` (`<ext>.<name>`, its `input_schema`); `wasm_owner` maps a tool name back to the extension and `host.rs` dispatches through `Workspace::run_wasm_command` (granted permissions from settings). Policy: unknown/third-party tools are `Mutating` → *Ask* unless `allow_writes`.
- Permissions gate the built-in write tools too: an agent extension without `write-files` granted denies `editor.replace` / `editor.propose` / `file.create`, without `run-commands` denies `terminal.run` (Settings → Extensions → Agent).

## Milestone 12
- `server_panel.rs`: `ServerAgentPanel` — used by the extension's `render` when `ws.agent_sessions()` is `Some` (the sources are a server's and `agent.on_server` is set, or the platform has no local provider). Sends through `AgentSessions::send`, polls `events(since)` every 700 ms while a turn runs (re-reading the last item, which may still be streaming; a final full fetch at the end), lists the folder's sessions (a running one is opened on arrival), answers approvals with `approve`. Root-scope signals, so the state survives panel remounts. `sleep_ms` (gloo-timers on wasm, tokio natively).
- `panel.rs`: the local agent's `cwd` is the first open folder (`Request.cwd`, what the `claude-code` provider runs in).

## Milestone 15 — sessions and saved agents
`panel.rs` holds **sessions**: `Chats` (a root context: `sessions`, `current`, the folder's `saved` summaries) and `Session` (id, root-owned signals for title, profile, items, agent, provider label, busy, pending, cited, audit). `new_session` picks the saved agent (`Settings::agent(name)`) and `connect`s a provider for it; `run_turn` runs the exchange in `spawn_forever` (the panel may close, the session may be switched away — the turn goes on; its approval waits in its own `pending`); `persist` writes `.moonkale/agent-sessions/local/<id>.json` (messages, items, profile, title, time) after each turn; `load_saved` lists that directory through `Workspace::list_at` (the folder source's `ls` dialect, P-118) when the folder changes; `restore` brings a saved one back and hands its messages to the new agent. Any number of sessions run at once; each runs one turn at a time. `server_panel.rs` gained the same profile select — `TurnSettings.llm` is the chosen profile's. The extension's settings section now holds the policy (allow writes, denied tools) and `on_server`; the language models are Settings → Agents. Pitfall met on the way: memos and renders must `read()`, not `peek()` (P-117).

## Milestone 18 phase 5.15
- `panel.rs`: `SavedSession` is a `moonkale_state::Record` (table `agent_sessions`); `sessions_key(folder)` = `str(folder id) · str("local")`. With a host store (`Workspace::has_host_state`) `persist` puts the session's row, `saved_sessions(ws, folder)` scans the folder's rows and imports the files of `SESSIONS_DIR` once when there are none, `restore` reads the row (the file as a fallback). Without one: the files, as before. Test: `tests/sessions_store.rs`.


WP-63 (2026-10-28): panel actions use `icons::ToolbarActions` for the Simple/Detailed density and the extension’s user-selected chrome. Secondary controls stay reachable through a flat ⋯ disclosure. Terminal session tabs and document/query content remain navigation/content. See [[ADR-0027 Simple by default]].

WP-46: local Chats state is retained per named outer session. Host records keep
the authorized folder/local prefix, then session identity; legacy rows migrate
to Default once. Server conversation heads/summaries carry outer_session, and
sending to a conversation belonging to another outer session is refused.


WP-70: display.rs validates typed display actions, budgets each conversation and queues quiet offers according to agent.open_views. Marked linked instances keep selection/ranges isolated; content renders through safe Markdown and existing table/graph views. Server events are ephemeral and checked on each client. WP-71 terminal.run uses the bounded private-gated ANSI relay tail.


### Proposals and review (2026-10-30)

Local text writes now queue in a bounded per-session Proposals inbox (`proposals.rs`). `editor.propose` groups existing files; `editor.replace` requires an expected read version too. Approval validates source versions and editor leases; file buffers apply as one group, filesystem saves report ordered results. Pending creation/command reviews from all local conversations use the same inbox. No roles or durable inbox protocol yet.
