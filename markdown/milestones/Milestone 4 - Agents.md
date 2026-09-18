---
title: "Milestone 4 — Agents: the plan"
description: An LLM as a user of the workspace — provider gateway, tool surface with a policy gate, an in-app agent panel, hybrid search over the index, and stack traces drawn as graphs.
tags: [milestone, planning]
---
**Goal** (from [[Roadmap]] Phase 4): *chat with an agent that queries your sources under policy; hybrid search; click a stack trace into a graph.* Record: [[Milestone 4 - Implementation Log]].

## Starting point (after Milestone 3)
- Sources: folder, index (wiki-links, Rust symbols), SQLite, LadybugDB; remote parity through `api` (server functions + typed websockets).
- Editors: code (CodeMirror + LSP), markdown source mode + Links + Typst preview, table (SQL/Cypher), graph (index/schema/data/query modes), terminal.
- `moonkale-llm` is comment-only (`provider`, `tools`, `policy`, `agent`, `audit`, `embed`). Design: [[LLM and RAG]].
- Machine: `ollama` binary present but no server and no models; no API keys in the environment. **Ask Daniel** for one of: `ANTHROPIC_API_KEY`, an OpenAI-compatible endpoint, or `ollama serve` + `ollama pull qwen2.5:1.5b nomic-embed-text` (~1.3 GB).

## Scope: what "done" means
On desktop and web:
1. **Provider gateway** — `Provider` trait (streamed completion with tool use; embeddings) with Anthropic (Messages API), OpenAI-compatible (chat completions; also serves Ollama's `/v1`), Ollama native embeddings, and a **mock** provider for tests. Config from environment; keys never reach the web client (the web build talks to `/api/llm` on the server).
2. **Tool surface + policy + audit** — tools `workspace.list_sources`, `graph.query`, `graph.fetch`, `source.text_query`, `index.search`, `editor.open`; each call classified `ReadOnly | Mutating | Destructive` (SQL via `sources-sql::text::classify`, Cypher by keyword) → allow / ask / deny; row and byte caps; every call in an audit log.
3. **Agent panel** — a chat with streaming text, tool-call cards, approval prompts for `ask` decisions, the active document as context, and **transcripts saved as markdown pages** in the folder (`.moonkale/chats/…md`, with wiki-links to everything cited) so they are indexed and appear in Links and the graph.
4. **Hybrid search** — the index chunks text files, ranks with BM25 and, when an embedding model is configured, with cosine similarity (reciprocal-rank fusion). Exposed as `Query::Text { dialect: "search" }` on the index source (web gets it through `RemoteSource` for free), a **Search panel** (Ctrl+Shift+F) whose results open the file at the line, and the `index.search` tool.
5. **Traces as graphs** — a parser for Rust panics/backtraces, cargo `-->` errors, Python tracebacks and JS stacks; a *Trace → Graph* action in the terminal (and paste box) builds an in-memory `TraceSource` (frames → files, call chain) the Graph panel draws; double-click opens the file at the line.
6. **Reveal** — `Workspace::reveal(node, line, col)`: opening a file at a position (search hits, trace frames, and the M3 gap: cross-file go-to-definition).

**Deferred** (documented, not started): Milkdown WYSIWYG (P-11 — rich text is its own milestone), Postgres/Turso (P-19, need servers), FalkorDB/TypeDB/Helix (P-26), an MCP server for external agents (decision recorded in the log: after the in-app tool surface has settled), secure remote terminal/LSP (P-20), LSP completion/rename.

## Architecture decisions for this milestone

```mermaid
flowchart LR
  subgraph client [client (desktop or web)]
    AP[editors/agent panel] --> AG[llm::agent loop]
    AG --> POL[llm::policy] --> TH[ToolHost over Workspace]
    TH --> SRC[(sources)]
    AG --> AUD[(audit)]
    AG --> PR{Provider}
  end
  PR -->|desktop| HTTP[Anthropic / OpenAI-compatible / Ollama]
  PR -->|web| WS[api ws: /api/llm] --> HTTP
  SP[Search panel] --> IDX[index: search = BM25 + vectors]
  TP[terminal: Trace → Graph] --> TR[trace crate → TraceSource] --> GP[Graph panel]
```

- **The agent loop runs on the client**, next to the workspace it acts on; only the *provider* is remote on web (`RemoteProvider` over a websocket, like `RemoteLsp`). Tools, policy and prompts stay in one place for both platforms.
- **Tools are executed by a `ToolHost` trait**, implemented by the agent extension over `Workspace`. `moonkale-llm` stays free of Dioxus so the same tools can later serve an MCP server on `api`.
- **Policy is source-informed**: raw statements are classified by the source family's classifier; structured queries are read-only by construction; `editor.open` is read-only; nothing mutating exists yet except transcript saving (which the user triggers, not the agent).
- **Search lives in the index** as a text dialect, not in a new crate: the index already owns the files, the chunks are a byproduct of walking them, and `RemoteSource` carries `Query::Text` unchanged. Embeddings are optional and cached by content hash; BM25 alone works offline.
- **Traces are a source** (`SourceFamily::Custom("trace")`) so the Graph panel's picker, popup and double-click work unchanged; frames are `Symbol`s, files are `File`s, edges `Contains` (file → frame) and `Calls` (frame → next frame). Opening a frame's file goes through the folder source by relative path.
- **Transcripts are files** (`Op::CreateText` added to `core`/`project-fs`): the cheapest way to make them nodes, indexable and linkable, without a new store.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | `llm`: types (`Message`, `ToolDef`, `Event`), `Provider` trait, `MockProvider`, Anthropic + OpenAI-compatible + Ollama (SSE streaming), env config | llm | unit: SSE parsers on recorded payloads; mock scripted turns |
| 2 | `llm`: tools + policy + audit; `api` `/api/llm` websocket + `RemoteProvider`; desktop provider from env | llm, api, desktop, web, mobile | unit: policy classification; check all targets |
| 3 | `core`/`project-fs`: `Op::CreateText`; `Workspace::reveal`; code editor honours it | core, project-fs, ext-api, editors/code | unit: create file; E2E: reveal |
| 4 | `editors/agent`: panel, `ToolHost` over `Workspace`, approval prompts, context, transcript save | editors/agent, ui | E2E with mock provider |
| 5 | `index`: chunks, BM25, embeddings (optional), `Query::Text{search}`; Search panel; `index.search` tool | index, editors/agent or ui | unit: ranking; E2E: search opens file at line |
| 6 | `trace` crate + `TraceSource`; terminal *Trace → Graph*; Graph panel picks trace sources | trace, editors/terminal, editors/graph | unit: parsers; E2E: trace graph node count |
| 7 | verify, log, vault; MCP decision recorded | | |

## Risks
| risk | mitigation |
|---|---|
| No API key / model on the machine | the mock provider drives all automated tests; Daniel tries a real provider once keys or Ollama models exist |
| Provider API drift (Anthropic/OpenAI streaming formats) | parsers are tolerant (unknown event types ignored) and unit-tested on recorded payloads |
| Context window blow-up | tool results are capped (rows, bytes) and summarised (ids + labels first) |
| Embeddings cost/time on big folders | opt-in via `MOONKALE_EMBED_MODEL`; cached by content hash; BM25 is the default |
| Agent writes | nothing exposes writes this milestone; the policy gate exists so the first write tool goes through *ask* |
