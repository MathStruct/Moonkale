---
title: "llm — implementation notes"
tags: [crate-notes, milestone-4]
---
Notes for `moonkale-llm` (Milestone 4). Design: [[LLM and RAG]].

Dioxus-free; everything but the HTTP providers compiles to wasm.

- `types.rs` — `Message { role, content: Vec<Content> }` with `Content::{Text, ToolUse{id,name,input}, ToolResult{id,output,is_error}}` (Anthropic's shape, the most general), `ToolDef` (JSON Schema), `Request`, streamed `Event::{TextDelta, ToolUse, Done{stop,usage}, Error}`.
- `provider.rs` — `trait Provider { name, model, complete(Request) -> EventStream, embed(Vec<String>) -> BoxFuture<Result<Vec<Vec<f32>>>>, supports_embed }`. Events arrive on an unbounded channel so the stream is `Send` whatever the provider does. `BoxFuture` is `Send` on native, not on wasm (same trick as `core`'s `async_trait(?Send)`).
- `anthropic.rs` (feature `http`) — Messages API with SSE streaming; `Translator` assembles `input_json_delta`s into one `ToolUse`. No embeddings.
- `openai.rs` (feature `http`) — chat completions with `tools`, streaming `tool_calls` deltas assembled by index; `/embeddings`. `OpenAi::ollama(host, …)` points at Ollama's `/v1` and uses its native `/api/embed`.
- `sse.rs` — incremental server-sent-events parser (both providers stream SSE).
- `mock.rs` — offline, deterministic: `/tool <name> <json>` in the chat makes it call that tool; after a result it says `Tool \`x\` returned: …`; otherwise it echoes. Embeddings are a hashed bag of words (32 dims). Drives every automated test.
- `config.rs` — `Config::from_env()`: `MOONKALE_LLM` (`anthropic|openai|ollama|mock`; default: whichever key/host is set, else mock), `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`/`OPENAI_BASE_URL`, `OLLAMA_HOST`, `MOONKALE_LLM_MODEL`, `MOONKALE_EMBED_MODEL` (unset = BM25-only search; the mock always embeds). `config::build` makes the provider.
- `tools.rs` — the six built-in `ToolDef`s (`workspace.list_sources`, `graph.query`, `graph.fetch`, `source.text_query`, `index.search`, `editor.open`) and result caps (12 k chars, 100 rows, 200 nodes).
- `policy.rs` — `Policy::decide(call) -> (Class, Decision)`: `source.text_query` is classified by `sources-sql::text::classify` (SQL) or a Cypher keyword scan; everything else is `ReadOnly`. Defaults: reads `Allow`, writes `Ask`, destructive `Ask` (never silently `Allow`); `denied_tools`.
- `audit.rs` — `AuditLog` of `AuditEntry { tool, input, class, decision, approved, ok, summary, millis }`.
- `agent.rs` — `Agent::send(text, &dyn ToolHost, on_event)`: model turn → tool calls through policy → `ToolHost::approve` for `Ask` → `ToolHost::call` → results back to the model, up to `max_tool_rounds`. `AgentEvent`s drive the panel; the transcript stays in `agent.messages`.

Tests: `cargo test -p moonkale-llm --features http` (SSE parser, both translators on recorded payloads, request bodies, policy classes, the loop with the mock).

## Milestone 5
- `LlmSettings` (in `types.rs`): provider kind, model, endpoint, embed model, **secret name** — what settings resolve to; `Config::from_settings(&LlmSettings, key)` builds the config. The web client sends `LlmSettings` to the relay with every call; the server resolves the secret.
- `secrets.rs` (native): `resolve(name)` = `MOONKALE_SECRET_<NAME>` → classic `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` → `<config dir>/moonkale/secrets.json` (0600, written by `store`); `config_dir()` (`MOONKALE_CONFIG_DIR` override). An OS keychain is the intended next step.
- Tools: `editor.replace`, `file.create`, `terminal.run` (see `tools.rs`); `policy::classify_command` marks `rm -rf`, `git push --force`, `sudo`, … as `Destructive` (always ask), other commands `Mutating`.

## Milestone 12
- `claude_code.rs` (feature `claude-code`, native): `ClaudeCode::new(path, model, permission_mode, allowed_tools)`; `complete` spawns `claude -p <prompt> --output-format stream-json --verbose --permission-mode <m> [--allowedTools …] [--model …] --session-id <new> | --resume <id>` in `Request.cwd` and maps the lines (`assistant` text → `TextDelta`, `tool_use` → a `▸ Name target` line, failed `tool_result` → `↳ error`, `result` → `Done`/`Error`, `rate_limit_event` rejected → a warning line; unknown kinds skipped). `plan_turn`: per-cwd `(session id, user turns)`; one more user message → `--resume`, else a new session whose first prompt carries the history. 10-minute turn timeout kills the CLI. `MOONKALE_CLAUDE_BIN` overrides the binary (tests: `tests/mock-claude.sh`, `tests/claude_code_mock.rs`). Never `--bare`.
- `types.rs`: `Request.cwd`, `LlmSettings.options` (`permission_mode`, `allowed_tools` for `claude-code`); `config.rs`: `ProviderKind::ClaudeCode`, `from_parts(.., &options)`.
- `sessions.rs`: the wire types of server-side agent sessions (`SessionItem`, `PendingApproval`, `SessionSummary`, `SessionState`, `TurnSettings`) shared by `api`, `ext-api` and the agent panel.
- `agent.rs`: `Agent.cwd`; `ToolOutcome` is serialisable.
