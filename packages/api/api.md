---
title: "api — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `api` (Milestone 1). Design: [[ADR-0005 Server functions as the remote backend]].

## Server functions
`open_folder(path)`, `list_sources()`, `query_source(source, q)`, `fetch_text_from(source, node)`, `apply_to(source, tx)` — all `#[post]`. The three data calls return `Result<Result<T, SourceError>, ServerFnError>` **on purpose**: the outer error is transport, the inner one is the remote source's own error, so a `Conflict` on the server arrives as a `Conflict` on the client and the editor's conflict UI works over the network unchanged.

## `RemoteSource` (`remote.rs`)
Holds a `SourceDescriptor`, implements `Source` by calling the functions above. Compiled on every client (it is what the web build uses); on the server it is simply unused.

## State (`feature = "server"`)
`static REGISTRY: OnceLock<SourceRegistry>`. `open()` canonicalises the requested path and refuses anything outside `MOONKALE_ROOT` (default: the server's cwd). Relative paths resolve under the root; blank means the root itself.

## Milestone 3: tools over websockets (dev-server only)
- `terminal.rs` — `#[get("/api/terminal")] terminal_socket` → `Websocket<TerminalMessage, TerminalMessage>`: first message `Open{cwd,cols,rows}` (cwd jailed by `state::jail_dir`), then a `PtyBackend` on the server relays `Input`/`Output`/`Resize`. Client: `RemoteTerminal: TerminalBackend`.
- `lsp.rs` — `#[get("/api/lsp")] lsp_socket` → `Websocket<Frame, Frame>` (`Frame(String)` newtype: the macro rejects bare `String` type parameters, P-057): first frame is JSON `{language, root}`; the server discovers and spawns the language server (`moonkale-lsp-local`) with the jailed root and relays JSON-RPC 1:1. Client: `RemoteLsp: LspTransport`. Errors before the relay starts come back as one `{"error": …}` frame.
- `compile_typst(root, rel, text)` — plain `#[post]` server function; `moonkale-typst` in-process.
- `open_any` now also opens `.lbug/.kuzu` databases (`moonkale-sources-graph::ladybug`).

**Security:** both sockets run a process on the server for whoever can reach the port, with no auth. `MOONKALE_ROOT` limits the cwd, not what the shell can do. Dev-server only until the Platform Matrix auth items are done.

## Critical decisions / gotchas
- **Naming (P-036):** a server function named `query` with a parameter `query` does not compile — the generated client stub calls the function by name and the parameter shadows it. Hence the `_source/_from/_to` suffixes.
- **No auth, no size cap.** This is a local dev server until the Platform Matrix security items are done. Large files go over the wire whole.
- Deps on `project-fs` and `sources` are `optional` and only enabled by the `server` feature on non-wasm targets, so the client build never sees them.

## Milestone 4: LLM relay
`llm.rs` — `#[get("/api/llm")] llm_socket` (`Websocket<Frame, Frame>`, one socket per call): the client sends `{kind: complete, request}` or `{kind: embed, texts}`, the server runs the provider it built from its own environment (`MOONKALE_LLM`, keys) and streams `{kind: event, event}` / `{kind: embedding, vectors}`. `llm_info` (`POST /api/llm/info`) reports name/model/embedding support. `RemoteProvider` (wasm only) implements `Provider` over these; `embedder()` gives the index the same provider when `MOONKALE_EMBED_MODEL` is set. Keys never reach the browser; the relay has no auth (dev server).

## Milestone 5
- `open_folder(path, embed: Option<LlmSettings>)`: the client's embedding settings decide whether the index embeds; the secret is resolved on the server.
- `/api/llm`: `Complete`/`Embed` frames carry `LlmSettings`; `provider_for(settings)` builds and caches a provider per settings; `llm_info(settings)` reports `has_key`.
- `mcp.rs` — `POST /mcp` (mounted by `web/src/main.rs` via `dioxus::server::router(App).route(...)`): JSON-RPC `initialize`, `ping`, `tools/list`, `tools/call` for the read-only tools (`workspace_list_sources`, `graph_query`, `graph_fetch`, `source_text_query`, `index_search`) over the server registry; mutating statements refused; optional `MOONKALE_MCP_TOKEN` bearer auth. Verified with Claude Code: `claude mcp add --transport http moonkale http://127.0.0.1:8080/mcp`.

## Milestone 6
- `wasm.rs` — `list_wasm_extensions(folder)` and `run_wasm_command(ext, command, args, granted)` server functions: the server's `ext-host` runtime (feature `wasmtime`) over the registry; the client sends the granted set from its settings (the server trusts the dev client — same trust level as the terminal relay, P-20). The browser build never compiles wasmtime.

## Milestone 7
- `git.rs` — `git_run(root, GitRequest)` server function: jails `root`, runs `moonkale_ext_git::cli::run`. `api` now depends on `ext-api` for the shared request/response types.
- `auth.rs` (feature `server`) — `MOONKALE_TOKEN`: middleware on every route (`protect(router)`): `/login`, `/assets/`, `/wasm/`, `/_dioxus`, favicon are public; `/api/*` and `/mcp` answer 401 without the `moonkale_token` HttpOnly cookie or `Authorization: Bearer`; the page redirects to `/login`; `/mcp` is left to its own `MOONKALE_MCP_TOKEN` when that is set. One `moonkale::audit` line per relay call (`<ip> <method> <path> auth=dev|token`). `/login` (GET form, POST sets the cookie; `Secure` when `X-Forwarded-Proto: https`) is rate-limited to 10 attempts/minute per `X-Forwarded-For` (or globally without a proxy). `guard_bind()` / `bind_mode(ip, has_token)`: no token + non-loopback `IP` → exit 2 before serving.

## Milestone 8
- `presence.rs` — `/api/presence` websocket (`PresenceMessage`): first message `Join { room, member }`; the hub keeps `HashMap<room, Room { members, broadcast }>` in memory, broadcasts the sorted member list on every upsert and on leave, drops empty rooms; `RemotePresence` is the client `PresenceLink` (queued updates, member lists to a callback, empty list when the socket closes).
- `wasm.rs` — `module_bytes` at `/api/ext/module/{id}` (plain axum route, `application/wasm`) for the browser runtime.
- `auth.rs` — `protect()` also adds `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` (needed for `SharedArrayBuffer`); `MOONKALE_ISOLATE=0` disables them.

## Milestone 11
- `client.rs` — the client half for builds without the `server` feature: `connect(url, token, label)` (`set_server_url` + `Authorization: Bearer` through `set_request_headers`), `disconnect()`, `active()`; the `WorkspaceConfig` callbacks over the server (`open_folder`, `attach_source`, `spawn_terminal`, `compile_typst`, `spawn_lsp`, `git`, `wasm_list`, `wasm_run`) shared by the web client and the desktop in remote mode; `session_token()` (32 random bytes, hex). The desktop's dispatchers check `active()`.
- `relay.rs` (native) — dioxus's server URL is a `OnceLock` set by `launch` (P-098), so the desktop calls `relay::install()` **before** launch and passes the returned `http://127.0.0.1:<port>` to `set_server_url`; a thread with a current-thread runtime + `LocalSet` accepts connections and `copy_bidirectional`s each one to `client::active()`'s host:port — `tokio-rustls` with `webpki-roots` when the URL is `https://`; refused when no remote is active. Server functions and typed websockets both go through it. Unit test: relays to an echo upstream, closes without one.
- `auth.rs` — `same_host(origin, host)` and a **403 on cross-origin websocket upgrades**; `tls_files()` (`MOONKALE_TLS_CERT` + `MOONKALE_TLS_KEY`); `bind_mode(ip, has_token, tls, insecure_ok)`: off loopback a token *and* TLS are required, `MOONKALE_INSECURE_HTTP=1` opts out behind a TLS proxy.
- `terminal.rs` — `MOONKALE_TERMINAL=0` answers every `Open` with `Exit { message: "the terminal is switched off on this server (MOONKALE_TERMINAL=0)" }` and an audit line.
- Verified by `packages/web/tests/e2e/server.mjs` against the standalone binary (token over stdin, 401/200, terminal off, cross-origin 403, plain-HTTP bind refused, HTTPS with a self-signed certificate) and `desktop/tests/remote.rs` (ignored; `MOONKALE_REMOTE=http://127.0.0.1:8090`).
