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

## Critical decisions / gotchas
- **Naming (P-036):** a server function named `query` with a parameter `query` does not compile — the generated client stub calls the function by name and the parameter shadows it. Hence the `_source/_from/_to` suffixes.
- **No auth, no size cap.** This is a local dev server until the Platform Matrix security items are done. Large files go over the wire whole.
- Deps on `project-fs` and `sources` are `optional` and only enabled by the `server` feature on non-wasm targets, so the client build never sees them.
