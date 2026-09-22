---
title: "Agent Sessions and Profiles"
description: How the Agent panel runs several agents at once — saved agents (language-model profiles) in settings, one chosen per session, sessions side by side with their history in the folder — locally and on the server; and how Claude Code logs in.
tags: [architecture, agent, llm, settings]
---
Built in [[Milestone 15 - Agents, Profiles and Connections]] from [[Prompt24]]. Background: [[LLM and RAG]], [[Claude Code Extension]].

## Saved agents (profiles)
A **saved agent** is a name plus a language model: provider, model, endpoint or command, secret name, provider options (Claude Code's permission mode and allowed tools). They live in settings:

```json
{
  "llm":    { "provider": "openai", "model": "mistral-code-latest", "base_url": "https://api.mistral.ai/v1", "secret": "mistral" },
  "agents": [
    { "name": "Claude", "provider": "claude-code", "options": { "permission_mode": "acceptEdits" } },
    { "name": "Local",  "provider": "ollama", "model": "qwen2.5-coder" }
  ],
  "agent":  { "default": "Claude" }
}
```

- The flat `llm` block **is** the profile called **Default** — every settings file, environment override (`MOONKALE_LLM`, `OPENAI_BASE_URL`, …) and test from before keeps its meaning. `agents[]` adds more. Both scopes may contribute; a workspace entry with the same name overlays the user one field by field.
- `agent.default` names the one that runs unless a session says otherwise; an unknown name means Default. The resolved `Settings.llm` is *the default agent's* settings, so search embeddings, server sessions and everything else that asked for "the LLM" are unchanged.
- The embedding model belongs to search, not to an agent: it is set once (Settings → Search) and served by Default's provider; every profile carries it.
- Keys are never in settings: the secret *name* is, the key comes from `MOONKALE_SECRET_<NAME>`, the classic environment variables or the secrets file ([[Security]]).

**Settings → Agents** shows one card per profile: name (Default's is fixed), *runs by default*, provider, model, endpoint/command, secret; a Claude Code card also shows the CLI's status and a **Log in** button. *Add agent* appends `Agent N` (mock) to edit; *Remove* forgets a card.

## Sessions
The Agent panel holds any number of **sessions**. Each has its own transcript, agent (`moonkale_llm::Agent` with the conversation), provider built from *its* saved agent, busy flag, pending approval and audit. The toolbar: the session select (`●` marks one with a turn in flight; below a line, the folder's saved sessions), **New**, the **profile select** for the current session (disabled while it runs), Activity, Save (as a markdown page), Clear, × (close; the session stays saved).

**Concurrency.** A session runs one turn at a time — its compose box is closed meanwhile — and any number of sessions run at once. A turn is a root-owned task (`spawn_forever`), so closing the panel, switching sessions or opening another file does not stop it; an approval waits in that session until you come back and answer. With Claude Code, each running session is its own `claude` process in the folder. The same rule holds on the server: one thread per turn, one turn per session ([[Remote and Server Modes]]).

**History.** When a turn ends, the session is written to `.moonkale/agent-sessions/local/<id>.json` — its messages (what the model saw), the rendered items, profile, title (the first message) and time. Opening a folder lists these under *saved in this folder* in the session select; picking one restores the transcript, reconnects its profile and continues the conversation. The folder source's `ls` query dialect lists that hidden directory (`Children` skips hidden and ignored entries on purpose). The server's own sessions (`agent.on_server`) are the append logs `<id>.jsonl` next to it — a different format, because the server records approvals and streams; a served folder lists those, not the local snapshots.

## Claude Code: status and login
The `claude-code` provider reports its readiness (`Provider::status`): `claude --version` and `claude auth status --json` → *not installed* (with the install command), *not logged in*, or *logged in as … (claude.ai)*. Settings shows it on the profile card, with **Check again**. **Log in** runs `claude auth login` as a terminal tab (`WorkspaceConfig::spawn_program`, desktop): the CLI opens the browser, you sign in and paste the code into that tab — the flow Daniel expected in [[Prompt24]], without Moonkale ever touching the credential. On the web the CLI runs and logs in on the server (`/api/llm/status` reports its state; the button is absent there).

## Saved SSH connections
*Open Remote Folder…* keeps connections in the user file (`remote.saved[]`: name, the host as typed after `ssh`, the folder): a select fills the fields, *Save* stores the current ones under a name, *Forget* drops one. The user file only — a workspace file lives inside the folder that is being opened remotely.

## Where things are
- `ext-api/settings.rs`: `AgentProfileFile`, `RemoteFile`/`SavedConnection`, `AgentProfile`, `DEFAULT_AGENT`, `Settings::agent(name)`, the merge (`overlay_llm`).
- `ext-api/workspace.rs`: `remote_saved/save_remote/forget_remote`, `spawn_program`/`run_in_terminal`, `write_text_at`/`list_at`.
- `editors/agent/src/panel.rs`: `Chats` (root context), `Session`, `new_session/connect/run_turn/persist/load_saved/restore`; `server_panel.rs`: the profile select feeding `TurnSettings.llm`.
- `llm/src/provider.rs`: `ProviderStatus`; `claude_code.rs`: `cli_status`; `api/src/llm.rs`: `llm_status`.
- `ui/src/settings_panel.rs`: `AgentProfileCard`, `ClaudeStatus`; `ui/src/remote_dialog.rs`: the saved-connection rows.
- Tests: `ext-api` unit tests (merge, default, connections), `project-fs` (`ls`), `llm` (`cli_status` on the mock CLI), `ui/tests/remote_dialog.rs` (harness), E2E `agents.mjs`.
