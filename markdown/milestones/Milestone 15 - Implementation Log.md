---
title: "Milestone 15 — Implementation Log"
description: What was built for "Agents, profiles and connections" — saved agents chosen per session, several sessions with turns in flight at once and their history in the Agent panel, saved SSH connections, Settings without the extension list, and a Log in button for Claude Code.
tags: [milestone, log, agent, settings, remote]
---
Plan: [[Milestone 15 - Agents, Profiles and Connections]]. Design: [[Agent Sessions and Profiles]].

> [!success] Done 2026-09-22 — 113 native tests, 38 browser suites
> **Saved agents**: Settings → Agents holds one card per profile (Default = the old flat `llm`, more under `agents[]`), one *runs by default*, and every Agent-panel session picks its own from a select. **Sessions run side by side**: each has its transcript, agent, provider, busy flag and approval; a turn is a root-owned task that survives closing the panel; *New* starts another while one works; finished turns are written to `.moonkale/agent-sessions/local/<id>.json` and listed under *saved in this folder* — pick one and it continues. **Settings lost its extension list** (the Extensions panel has the checkboxes); it keeps the *Which extension* selectors (code editor, terminal); the agent's policy and *run on the server* moved under the Agent extension; the two editors' and terminals' copies of the implementation select went away. **Saved SSH connections** in *Open Remote Folder…* (select, Save as, Forget; user file). **Claude Code**: the profile card shows `claude 2.1.278 · logged in as … (claude.ai)` (or *not installed* with the install command, or *not logged in*), **Log in** opens `claude auth login` as a terminal tab — the browser opens, the code is pasted there — and *Check again* re-probes.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | Settings model: `AgentProfileFile` (`name` + flattened `LlmFile`), `agents[]`, `agent.default`, `RemoteFile{saved[]}`; resolution → `Settings.agents` (Default first), `Settings.llm` = the default agent's, `Settings.agent(name)`; same-name profiles overlay field-wise across scopes (`overlay_llm`) | ✅ 2 unit tests (`saved_agents_merge_by_name_and_one_is_the_default`, `saved_connections_come_from_the_user_file`); old files parse unchanged | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md) |
| 2 | Settings panel rewritten: Agents (cards: rename, runs-by-default radio, provider, model, endpoint/command, secret, Claude Code knobs + `ClaudeStatus`; Add / Remove), Search (embedding model + toggle), Which extension (editor, terminal), You, Keybindings, Remembered; the Extensions list and the policy left; the Agent extension's section gained the policy; `editor-code`, `editor-code-native`, `editor-terminal`, `editor-terminal-native` dropped their implementation selects | ✅ `settings.mjs`, `extensions.mjs`, `flow.mjs`, `wasm-ext.mjs` updated (they enabled extensions through Settings) | |
| 3 | Agent panel sessions: `Chats` as a root context (`provide_root_context`), `Session` with root-owned signals, `new_session / connect / run_turn / persist / load_saved / restore`, session select with `●`, *New*, profile select, ×; `Item` serialisable; server panel gets the profile select (`TurnSettings.llm` from `Settings::agent`) | ✅ `agents.mjs`: Add agent → user file; runs-by-default; profile select reconnects; **A waits for an approval while B answers** (`● ` on A); back to A → Allow → output; two snapshot files; reload → *saved in this folder* → restore → continue | |
| 4 | Persistence plumbing: `Workspace::{write_text_at, list_at}`; the folder source's **`ls` query dialect** (hidden, ignored entries included — `Children` skips `.moonkale/` by design) | ✅ `project-fs` test `ls_dialect_lists_hidden_directories` | P-118 |
| 5 | Claude Code: `Provider::status` + `ProviderStatus`; `claude_code::cli_status` (`--version`, `auth status --json`, 15 s timeout, `CLAUDECODE` unset); `api::llm_status` server fn (web shows the server's state, no button); `WorkspaceConfig::spawn_program` (desktop: `PtyBackend::spawn_args`) + `Workspace::run_in_terminal` → *Log in* tab; mock CLI answers `--version` and `auth status` | ✅ `cli_status_reads_version_and_login` (mock), real CLI here: `claude 2.1.278 · logged in as dn.boigk@gmail.com (claude.ai)`; `claude auth login` under a PTY prints the URL and `Paste code here if prompted >` | |
| 6 | Remote dialog: saved-connection select, *Save* (name), *Forget*; `Workspace::{remote_saved, save_remote, forget_remote}` | ✅ `ui/tests/remote_dialog.rs` (VirtualDom harness: save → user file + select appears; pick + submit → `open_remote` gets the saved host/path) | |
| 7 | Verify, log, vault | ✅ 113 native, 38 browser suites (`agents` added to `run-all.sh`); this note; [[Agent Sessions and Profiles]]; [[Extension Catalogue]], [[LLM and RAG]], [[Getting Started]], Home/README; P-117, P-118 | |

## The Claude Code question
"Claude Code does not really run" — on this machine the provider does run (a headless turn from the app's exact command line answered in 2 s), and the CLI was already logged in. What was missing was everything *around* it: the app never said whether `claude` existed or was logged in, and there was no way to log in from the app. Now the profile card says which of the three states holds, and *Log in* is the CLI's own OAuth flow (browser + pasted code) in a terminal tab — Moonkale never sees the credential ([[Security]]). If it still does not run for you, the card's line is the first thing to read: *not installed* means the desktop's `PATH` lacks `claude` (put the full path into *Command*); *not logged in* means the button; *logged in* and still failing means the CLI's own error, which now lands in the transcript's error bubble.

## What the user sees
- **Settings → Agents**: cards; *Add agent*; the Claude Code card's status line and buttons. Keys unchanged (secret names + the secrets box).
- **Agent panel**: `[session ▾] [New] [Claude ▾] provider · model  … Activity Save Clear ×`; `2 running` when more than one turn is in flight; *saved in this folder* below the live sessions.
- **Extensions panel**: the Agent row has *Allow mutating tools…*, *Denied tools*, *Run turns on the server*.
- **File → Open Remote Folder…**: *Saved connection* select (when any), a name field with *Save*, *Forget*.

## Deviations from the plan
1. **No per-profile policy** (as decided): one policy per scope, on the Agent extension.
2. **The concurrency demo uses an approval, not a timer**: the mock provider answers instantly, so `agents.mjs` parks session A on a `terminal.run` approval while B answers. A slow-provider test would need a timer in the mock; not added.
3. **The desktop's Settings card was checked through the provider label and the CLI probe, not a screenshot of the card**: the app opened with the *Claude* profile selected (`claude-code · subscription` in the toolbar), but reaching Settings needs a click (no input injection on the desktop). Daniel's look is the visual check.

## Problems hit (→ [[Problem Log]])
- **P-117** a `use_memo`/render reading `sources.peek()` never re-ran — the saved sessions were never listed after the folder opened, and the first session did not render. `peek` is for callbacks; `read` in anything that must follow the value.
- **P-118** `Query::Children` skips hidden and ignored entries, so nothing under `.moonkale/` could be listed → the `ls` dialect.

## Numbers
- `editors/agent/panel.rs` 640 → 830 lines; settings panel 255 → 420; `ext-api/settings.rs` +140.
- Tests: 113 native (+5), 38 browser suites (+1).
