---
title: "ADR-0013 — A JSON ABI over core wasm modules before components"
tags: [adr, extensions]
status: accepted
date: 2026-10-01
---
**Status:** accepted — records what Milestone 6 and 8 built; amends [[ADR-0004 WASM components for extensions]], which stays the target.

## Context
[[ADR-0004 WASM components for extensions]] chose WASM *components* with a WIT world generated from `ext-api`. When the runtime was built (Milestone 6) the component toolchain was the risky part: `cargo component`, a browser host for components (`wasm_component_layer`/`jco`), and a WIT world that would have frozen an `ext-api` still changing every milestone. A runtime was needed to prove permissions and agent tools, not to freeze the API.

## Decision
Version 1 of the wasm extension interface is a **JSON ABI over core wasm modules** (`moonkale-ext-host`, `ABI_VERSION = 1`):
- the module exports `alloc`, `manifest` and `run`; it imports `moonkale.log` and `moonkale.call`;
- every value crossing the boundary is JSON: `WasmManifest { abi, id, name, description, permissions, commands }`, `RunRequest`/`RunReply`, `HostCall`/`HostReply`;
- host calls: `list_sources`, `query`, `fetch_text`, each checked against the granted permissions;
- a module contributes **commands only** (which may be agent tools); no panels, no `ui::Tree`;
- runtimes: wasmtime on desktop and the server; a Worker with a `SharedArrayBuffer` mailbox in the browser (`packages/js/wasm-host`); none on Android yet.

Built with plain `cargo build --target wasm32-unknown-unknown` — no component tooling. Example: `packages/extensions/wordcount`.

## Consequences
- Third-party extensions are possible today, but only as commands/tools; anything with UI is a static crate compiled into the binary.
- The manifest is returned by the module at runtime; there is no `moonkale.toml` file yet ([[Manifest Reference]] describes the target).
- Permissions are only as strong as where they are checked: on the server path they come from the client (issue #4, [[Audit 2026-09-23]]); there are no fuel or memory limits.
- Moving to components later changes the packaging of a module, not what it may do: the host calls and permission names are the parts to keep.
