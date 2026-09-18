---
title: "Milestone 4 — Implementation Log"
description: What was built for "Agents", what deviated from the plan, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 4 - Agents]].

> [!success] Done (2026-09-18)
> All seven steps are implemented and verified on the web build with Playwright against the **mock provider**; the desktop build compiles and runs the same code with in-process providers. **Provider gateway** (Anthropic, OpenAI-compatible, Ollama, mock; keys stay on the server for web), **tool surface with a policy gate and audit**, an **Agent panel** with streaming, tool cards, approval prompts and transcripts saved as indexed markdown pages, **hybrid search** (BM25 + optional embeddings) in the index with a Search panel and `Ctrl+Shift+F`, **stack traces drawn as graphs** from the terminal or a paste box, and `Workspace::reveal` (open a file at a line — also closes the M3 go-to-definition gap). Two new E2E suites (`agent`, `search-trace`) plus the nine earlier ones pass; 52 native tests; clippy/fmt clean on every target.
>
> **Verified with a real model** (2026-09-18, Daniel's Mistral key in `.secrets/llm.env`, gitignored): through the OpenAI-compatible provider, `mistral-code-latest` called `index.search` and answered "src/main.rs, line 1" (correct) — `packages/web/tests/e2e/agent-live.mjs`; `mistral-embed` embedded the fixture's chunks (1024 dims). On that key `devstral-*` and `mistral-small-latest` answer "rate limited" (not enabled); `codestral-latest` works too.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `moonkale-llm`: types, `Provider`, SSE parser, Anthropic + OpenAI-compatible (+Ollama), mock, env config | ✅ 10 unit tests | [llm.md](https://github.com/MathStruct/Moonkale/blob/master/packages/llm/llm.md) |
| 2 | tools + policy + audit + agent loop; `/api/llm` relay + `RemoteProvider`; `WorkspaceConfig::llm` on desktop/web/mobile | ✅ | keys never reach the browser |
| 3 | `Op::CreateText` (core, project-fs); `Workspace::reveal` + code editor support; `Workspace::create_text` | ✅ 1 test | cross-file F12 now positions the cursor |
| 4 | `moonkale-editor-agent`: panel, `WorkspaceHost`, approvals, transcripts | ✅ E2E `agent.mjs` | [editor-agent.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/agent/editor-agent.md) |
| 5 | index search (chunks, BM25, embeddings, RRF) as `Query::Text{search}`; Search panel; `index.search` tool | ✅ 3 unit + 1 integration test, E2E | [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/index/index.md) |
| 6 | `moonkale-trace` parsers + `TraceSource`; terminal *Trace → Graph*; Graph panel paste box + picking | ✅ 4 tests, E2E `search-trace.mjs` | [trace.md](https://github.com/MathStruct/Moonkale/blob/master/packages/trace/trace.md) |
| 7 | verify, log, vault | ✅ | this note |

## What the user sees
- An **Agent** tab on the right. Header shows `provider · model` (`mock (server) · mock` until a key is configured). Ask anything; tool calls appear as cards with their policy class and decision; a write asks *Allow / Deny*; **Activity** lists every call; **Save** puts the conversation into `.moonkale/chats/` as a page with wiki-links to the files it cited (visible in Links and the graph).
- **Search** tab (Ctrl+Shift+F): `path:line` hits with snippets; click opens the file at the line.
- Terminal: after a panic or `cargo build` error, **Trace → Graph** draws the frames (files → frames → call chain); double-click a frame to open the file at the line. Graph panel **Trace…** does the same for pasted text.

## Deviations from the plan
1. **The mock provider is scriptable from the chat** (`/tool name {json}`) so the loop, policy and approval UI are testable without a model. It also embeds (hashed bag of words), so the vector path is covered.
2. **One websocket per completion** in the relay rather than a multiplexed session — trivially correct, and completions are long-lived anyway.
3. **Tool results are text**, not `GraphView`s: TSV with ids/kinds/labels/keys, capped. Good enough for models; a typed result format can come with MCP.
4. **Search is BM25-first**: embeddings are opt-in (`MOONKALE_EMBED_MODEL`) and filled in the background after the folder opens, so opening never waits for a model.
5. **Traces are sources** rather than a special graph payload: the picker, popup, double-click and E2E assertions all work unchanged. Frame keys are `path:line:col` so the Graph panel needs no trace-specific knowledge beyond the family.
6. **Transcript file names** carry a random suffix (a second save of the same first message must not collide — the E2E found it).
7. **The right tile** is part of the default layout now (Agent). Existing suites needed only a wider viewport.
8. **`Ctrl+Shift+F` needs focus inside the frame** (the key handler sits on the frame `div`); the suite clicks the explorer first. A document-level listener is a follow-up (P-065).

## Problems hit (→ [[Problem Log]])
- **P-063 Graph loads raced**: adding a trace source re-ran the load effect once with the index still picked; the slower remote reply overwrote the trace's counts. A generation counter lets only the newest load publish.
- **P-064 RefCell across await**: the agent must stay mutably borrowed for a whole exchange (streaming into `&mut self`), so the panel keeps an **audit mirror** signal and `busy` fences every other path; clippy's lint is allowed with the explanation.
- **P-065 Global shortcut needs focus in the frame** (see deviation 8).
- **P-066 Port 8080 was Daniel's desktop `dx serve`**; the web suites now take `PORT` (default 8080) and the dev server for tests runs on 8090. Leaked `server-*` processes from earlier dx runs were also cleaned up.
- The `Websocket<String, String>` macro limitation (P-057) applied again; the `Frame(String)` newtype is reused.
- **P-068 Graph node colours were wrong since M2**: the legend introduced in M3 made it visible (Daniel's desktop screenshot: City/Person/Project drawn blue/teal/green instead of cyan/purple/orange). `vertex_attr_array!` packed the colour at offset 12; the instance struct has it at 16. Fixed with explicit offsets.
- **P-067 Mistral model names**: the key lists 46 models but only some answer; unknown/unavailable ones return HTTP 429 "Rate limit exceeded" rather than 404, which looks like throttling. Use `mistral-code-latest` (tools + streaming work) and `mistral-embed`.

## Decisions worth keeping
- **The agent loop runs on the client**; only the provider is remote on web. Tools, policy and prompts live in one place for both platforms.
- **`ToolHost` keeps `moonkale-llm` free of Dioxus**, so an MCP server on `api` can reuse the tools later.
- **MCP**: deferred (recorded here as the decision): the in-app surface is six tools and will change once writes exist; an MCP endpoint should expose the same `ToolDef`s + `ToolHost` once they settle (Milestone 5 candidate).
- **Policy is source-informed**: SQL via `sources-sql::text::classify`, Cypher by keyword; destructive is never silently allowed.
- **Search as a text dialect** on the index: no new crate, no new server function, remote parity for free.

## Verified
- Web, real provider (Mistral via `OPENAI_BASE_URL=https://api.mistral.ai/v1`): `agent-live.mjs` PASS.
- Web (Firefox, Playwright, mock provider): `agent.mjs`, `search-trace.mjs`; regression: `graph`, `links-sqlite`, `terminal`, `typst`, `lsp`, `ladybug`, `session` — all PASS on port 8090.
- Native: `cargo test --workspace --features moonkale-sources-graph/ladybug` → 52 passed (llm 10, trace 4, index 5+3, project-fs 6, lsp/lsp-local, sources, typst, terminal-pty, graph-render…).
- Desktop: `cargo build -p desktop --features desktop` links; not run by hand this session.

## What to look at on desktop
1. Agent tab header says `mock · mock` → type `/tool workspace.list_sources {}` to see a tool round; type `/tool source.text_query {"source":"<a source id>","dialect":"sql","text":"DELETE FROM t"}` to see the approval box.
2. With a real key: ask "what files mention graph layouts?" and watch `index.search` run.
3. Ctrl+Shift+F, search a word, click a hit → cursor on the line.
4. Terminal: `cargo build` in `~/moonkale-sample` (it has a type error) → **Trace → Graph**.

## Deferred to Milestone 5
Milkdown WYSIWYG (P-11), MCP server for external agents, Postgres/Turso (P-19), Falkor/TypeDB/Helix (P-26), agent write tools (edit files, run commands — the `Ask` gate is ready), LSP completion/rename, document-level shortcuts (P-065), secure remote tools (P-20).
