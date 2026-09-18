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
