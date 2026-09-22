---
title: "Milestone 15 — Agents, profiles and connections: the plan"
description: Saved agents (language-model profiles) with one chosen per session, several sessions running at once with their history in the Agent panel, saved SSH connections in a dropdown, extension settings only with the extension, and a Log in button for Claude Code. Record in Milestone 15 - Implementation Log.
tags: [milestone, planning, agent, settings, remote]
---
From [[Prompt24]]. Record: [[Milestone 15 - Implementation Log]]. Background: [[LLM and RAG]], [[Claude Code Extension]], [[Remote and Server Modes]].

## What was asked, read carefully
1. The Extensions panel already has the on/off checkboxes — Settings must not repeat them. Settings keeps the *selectors* that pick one extension among several for the same job (which code editor, which terminal). Everything else an extension owns is under the extension.
2. Several SSH connections, saved, in a dropdown.
3. Several **saved agents** (a name plus a provider, model, endpoint, secret, options), even if only one runs; pick which saved agent a session runs.
4. **Several agents running at once** — document it; implement it if it is not too much hassle. (It is not: the local panel's one `busy` flag is the only thing in the way; the server side already runs one thread per turn.)
5. The **history of sessions** is part of the Agent panel.
6. **Claude Code "does not really run"**: the expected flow is *open the browser, sign in, paste the code into a dialog* — what `claude auth login` does in a terminal. Today the app assumes the CLI is already logged in and says nothing useful when it is not.

## Scope
1. **Settings model** (`ext-api/settings.rs`, `moonkale-llm`): `agents: [{name, provider, model, base_url, secret, options}]` in either scope (merged by name, workspace wins), `agent.default` = the name that runs unless a session says otherwise; the old flat `llm` fields stay and are the profile called **Default** (no migration, old files keep working); the resolved `Settings.llm` becomes *the default profile's settings*, so search embeddings, server sessions and everything else are unchanged. `remote.saved: [{name, host, path}]` (user scope). Unit tests for the merge and the default.
2. **Settings panel**: *Agents* (the profile list — add, remove, edit in place, "runs by default"; a Claude Code profile shows the CLI's status and a **Log in** button), *Search* (embedding model moves here from Language model), *Which extension* (code editor, terminal — the only extension-related thing left in Settings), You, Keybindings, Remembered. The Extensions list and the agent policy leave Settings: policy and `on_server` go to the Agent extension's own section in the Extensions panel; the editor and terminal extensions drop their copies of the implementation select.
3. **Agent panel, local mode**: sessions. Each session has its own transcript, agent, provider, busy flag and pending approval; a turn runs in a root-owned task (`spawn_forever`) so closing the panel does not kill it; the toolbar gets the session list (`●` = running) and *New*, a **saved-agent select** per session (default = `agent.default`), and the existing Activity / Save / Clear. Finished turns are written to `.moonkale/agent-sessions/local/<id>.json` (messages, items, profile, title, time); opening a folder lists them, picking one restores it. Same select on the server panel (a `TurnSettings.llm` per send).
4. **Claude Code**: `Provider::status()` (default `None`); the `claude-code` provider runs `claude --version` and `claude auth status --json` → *not installed* (with the install command) / *not logged in* / *logged in as …*. `WorkspaceConfig.spawn_program` (desktop: a PTY running a program with arguments) and `Workspace::run_in_terminal(title, program, args)` → the Log in button opens `claude auth login` as a terminal tab: the CLI opens the browser, the user pastes the code there. The provider's start-up error names the status instead of guessing.
5. **Remote dialog**: saved connections as a select (fills host and path), *Save as* with a name, *Forget*; `Workspace::remote_saved()` / `save_remote()` / `forget_remote()`.
6. Verify: unit tests (settings), a `VirtualDom` harness test for the remote dialog's save (desktop-only UI, like `ui/tests/server_dialog.rs`), E2E `agents.mjs` (profiles in Settings; two sessions in flight — one waiting for an approval, the other answering —; history restored after a reload; the profile select changes the provider label), `settings.mjs` and `extensions.mjs` updated for the moved sections, `claude-code.mjs` unchanged. Log, [[Problem Log]], [[Extension Catalogue]], [[Getting Started]], Home/README, site.

## Decisions
- **The flat `llm` stays and is the Default profile.** Every existing settings file, env override (`MOONKALE_LLM`, `OPENAI_BASE_URL`, …) and test keeps its meaning; profiles are additive.
- **A profile is a `LlmSettings` with a name** — no separate policy per profile; the policy (allow writes, denied tools) stays one per scope, on the Agent extension. A per-profile policy is a later step if wanted.
- **Local sessions persist as JSON, one file per session**, under `.moonkale/agent-sessions/local/` — next to the server's `<id>.jsonl` but not the same format (the server's is an append log with approvals; the local one is a snapshot written when a turn ends). A served folder therefore lists the server's sessions, not the local ones; documented.
- **Concurrency is per session**: a session runs one turn at a time (its input is disabled while it runs), any number of sessions run at once. Same rule as the server. A Claude Code profile spawns one `claude` per running session.
- **Login happens in a terminal tab, not a bespoke dialog.** The CLI already opens the browser and prompts for the code; a dialog would have to parse its output and break on the next CLI version. The tab is the dialog.
- **Saved connections live in the user file only** — a workspace file is inside the folder that is being opened remotely; it cannot hold the way to reach it.

## Risks
| risk | mitigation |
|---|---|
| a turn running while its session file is being written | the snapshot is written after `send` returns, from the agent's own messages; nothing else writes that file |
| two sessions editing the same document through tools | approvals are per session and shown in that session's transcript; the document model was already multi-writer (history, presence) |
| `claude auth status` absent on an older CLI | treated as *unknown*, the Log in button still works (`claude` alone shows `/login`) |
| profiles referenced by name but renamed | the session keeps the name it started with; a missing name falls back to Default with a note in the provider label |
