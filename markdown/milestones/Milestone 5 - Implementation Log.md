---
title: "Milestone 5 — Implementation Log"
description: What was built for "Settings & Writing", what deviated from the plan, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 5 - Settings and Writing]].

> [!success] Done (2026-09-18)
> All eight steps are implemented and verified on the web build; desktop compiles and shares every code path except the stores. **Settings** persist in two scopes (user: `~/.config/moonkale/settings.json` or `localStorage`; workspace: `.moonkale/settings.json` in the folder) with environment overrides on top; the app now **remembers** the layout per folder, the open documents and the active one, recent folders (File → Open Recent), the provider/model/endpoint choice and policy overrides. A **Settings panel** (`Ctrl+,`, File → Settings…) edits either scope with scope badges and a JSON view, and stores API keys into the desktop secrets file. **Rich markdown** (Milkdown/Crepe) sits behind a Source | Rich toggle on every `.md` tab, round-trips wiki-links and opens them on Ctrl+click. The **agent can write**: `editor.replace` (diff card → unsaved edit in the editor), `file.create`, `terminal.run` (output back to the model; destructive patterns always ask). An **MCP endpoint** (`POST /mcp`) gives external agents read-only tools — verified with Claude Code itself. Three new E2E suites plus the nine earlier ones pass; 56 native tests; clippy/fmt clean.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `ext-api::settings`: `SettingsFile` (persisted, all optional) / `Settings` (resolved), overlay order, env overrides; `Workspace` signals + loaders/savers | ✅ 2 unit tests | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md) |
| 2 | stores: desktop XDG file + secrets file, web `localStorage`, mobile none; workspace file through the folder source (`Query::Text{path}` dialect added) | ✅ 1 unit test | [llm.md](https://github.com/MathStruct/Moonkale/blob/master/packages/llm/llm.md) (`secrets`), [project-fs.md](https://github.com/MathStruct/Moonkale/blob/master/packages/project-fs/project-fs.md) |
| 3 | remembered: layout (workbench `on_layout_change` → workspace file; restored on open), open documents + active, recent folders, provider + policy | ✅ E2E `settings.mjs` | Reset Layout clears the saved one |
| 4 | Settings panel with user/workspace target, scope badges, JSON tab, secret store | ✅ E2E | [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 5 | Milkdown: `js/milkdown` (Crepe) bundle, `RichTextBackend` + `RichPanel`, Source \| Rich on `.md` tabs, wiki-link Ctrl+click | ✅ E2E `rich.mjs` | [markdown.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/markdown/markdown.md) |
| 6 | agent writes: `editor.replace`, `file.create`, `terminal.run`; command classification; diff/command cards | ✅ 1 unit test, E2E `agent-writes.mjs` | [editor-agent.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/agent/editor-agent.md) |
| 7 | MCP: `api::mcp` (`initialize`, `tools/list`, `tools/call`, read-only), mounted at `/mcp` by the web server | ✅ curl + Claude Code (`claude mcp add --transport http`) | [api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/api/api.md) |
| 8 | verify, log, vault | ✅ | this note |

## What the user sees
- Launch the desktop app: the last folder reopens with the same tabs, layout and active file (or use File → Open Recent).
- `Ctrl+,`: choose the provider (mock / Anthropic / OpenAI-compatible / Ollama), model and endpoint; pick where a change goes (this machine or this folder); paste an API key once into *Store secret* — it lands in `~/.config/moonkale/secrets.json` (mode 600), never in settings. Tick *Allow mutating tools* for a trusting session.
- Every `.md` tab has **Source | Rich**. Rich mode is a WYSIWYG editor whose output is still markdown; Ctrl+click on a wiki-link (double square brackets) opens the page.
- The agent proposes edits as red/green diff cards and commands as `$ …` cards; *Allow* applies the edit to the open editor (unsaved) or runs the command in a real terminal session and hands the output back to the model.
- `claude mcp add --transport http moonkale http://127.0.0.1:8080/mcp` gives Claude Code read-only tools over the folder the web server has open.

## Deviations from the plan
1. **No OS keychain yet**: `keyring` needs a Secret Service on Linux; the secrets file (0600) plus `MOONKALE_SECRET_<NAME>` / classic env variables cover the need. Recorded as the next step in `llm::secrets`.
2. **Server-side providers are built from the client's settings** (`ClientMsg::{Complete, Embed}` carry `LlmSettings`; cached per settings) — the secret is still resolved on the server, so the browser never sees a key. `llm_info` reports whether the named secret exists.
3. **Embeddings for the index follow the client's settings** via `OpenOptions { embed }` passed through `open_folder` on every platform (the server function gained a parameter).
4. **Milkdown escapes the wiki-link brackets** on output; the bundle un-escapes that pattern on the way out (`unescapeWiki`). Other remark reformatting (e.g. `\_`) is accepted as the cost of rich mode; Source stays canonical.
5. **The markdown extension owns `.md` tabs** (`CodeEditorExtension::skipping(is_markdown)`) and hosts the CodeMirror panel in Source mode — the panel id scheme (`editor:<uuid>`) is shared so tabs, closing and `active_panel` behave the same.
6. **`editor.replace` is exact-match** (`old` → `new`, must be unique) rather than line ranges: robust for models, and the change lands in the open `Document` so undo/dirty/save all apply and the file on disk is untouched until the user saves.
7. **`terminal.run` ends the shell** (`cmd; exit $?`) so the output stream closes; interactive commands would hang (no timeout on wasm yet — P-069).
8. **MCP is the JSON-only subset** of streamable HTTP (one request → one JSON response; notifications get 202; no SSE stream, no sessions). Enough for Claude Code; tool names use `_` because clients validate `^[a-zA-Z0-9_-]+$`.
9. **Global shortcuts** now also work with nothing focused: a document-level listener forwards Ctrl-combos whose target is `body` (closes P-065).

## Problems hit (→ [[Problem Log]])
- **P-072 The last folder was not reopened at launch** (Daniel's first desktop test): restore ran only after a manual open. Desktop now reopens `recent_folders[0]` at start (`reopen_last_folder`); tracing added on the restore path. Retest pending — my own desktop runs coincided with a locked KDE session (WebKit suspends invisible pages), so they were inconclusive.
- **P-069 `terminal.run` has no timeout** on wasm (no timer primitive without a JS dependency); output is capped at 200 KB and the shell exits after the command, but an interactive command would wait forever. Follow-up: a cancel button on the tool card.
- **P-070 `dx serve` kept serving a stale JS asset** after `npm run build` rewrote `milkdown.js` (the content hash in the page did not change); restart dx after rebuilding a bundle.
- **P-071 Hydration shows server-side settings for a moment**: the Settings panel's scope badge is computed from `env_overrides()`, which differs between server (has env) and wasm (none) until the first client re-render. Cosmetic; same family as P-034.
- **P-055 again**: headless Firefox drops modifier keys on `page.mouse.click`; the rich-mode suite dispatches the Ctrl+click.
- The mock's echo was widened to 1500 chars so tests (and people) can read tool results through it.

## Decisions worth keeping
- **Files are the settings store**; `.moonkale/settings.json` is committable and indexed like any file. `SettingsFile` is all-optional so a scope overrides only what it sets; `version` guards the format.
- **Secrets are names in settings, values elsewhere** (env → classic env → secrets file). The web client passes settings, never keys.
- **`Query::Text { dialect: "path" }`** on the folder source resolves hidden paths without a tree walk — used for the workspace file and by `Workspace::node_at_path`.
- **Agent writes go through the document model**, never the filesystem; the user's Save is the commit.
- **External agents are read-only** over MCP by policy class, not by a separate tool list.

## Verified
- Web (Firefox, Playwright, mock provider, port 8090): `settings`, `rich`, `agent-writes` (new) + `agent`, `search-trace`, `graph`, `links-sqlite`, `ladybug`, `terminal`, `typst`, `lsp`, `session` — all PASS.
- MCP: `initialize` / `tools/list` / `tools/call` via curl; Claude Code (`claude -p` with the server added) listed sources and searched the index correctly; a `DELETE` over MCP is refused.
- Native: 56 tests pass; desktop builds.

## What to look at on desktop
1. `~/.config/moonkale/settings.json` appears after the first change in Settings (user target); `<folder>/.moonkale/settings.json` after moving a tab.
2. Settings → provider *OpenAI-compatible*, endpoint `https://api.mistral.ai/v1`, model `mistral-code-latest`, secret name `openai` → *Store secret* with the key → the Agent header switches to `openai · mistral-code-latest` without a restart.
3. Open a `.md`, click **Rich**, edit, Save; check the file.
4. Ask the agent to "rename the heading in README.md to Hello" → diff card → Allow → the editor shows the change unsaved.

## Deferred to Milestone 6
GPU layouts + 100k nodes (P-22), wasmtime extensions with the permissions UI (P-23), flow editor + Lux.jl (P-24), mobile shell (P-25), Postgres/Turso (P-19), TypeDB/Helix (P-26), OS keychain, a cancel for running tools (P-069), MCP over SSE/sessions if a client needs it.
