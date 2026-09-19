---
title: "ext-host — implementation notes"
tags: [crate-notes, milestone-6]
---
Notes for `moonkale-ext-host` (Milestone 6). Design: [[Extension System]], [[ADR-0004 WASM components for extensions]], [[WASM Extension Runtimes]].

**What it is**: the runtime for *third-party* extensions shipped as **core wasm modules** with a JSON ABI. Built-in extensions are static Rust crates registered in `ui::default_extensions()`; their enablement lives in `ext-api::settings` (not here), so this crate is only about sandboxed modules.

- `abi.rs` — the shapes crossing the boundary, shared with guests written in Rust (depend on this crate with no features): `ABI_VERSION = 1`, `WasmManifest { abi, id, name, description, permissions, commands }`, `WasmCommand { id, title, description, input_schema, llm_tool }`, `RunRequest { command, args }`, `RunReply { ok, result | error }`, `HostCall::{ListSources, Query{source, query}, FetchText{source, node}}` (each maps to a permission — all `read-sources` today), `HostReply`.
- `runtime.rs` (feature `wasmtime`, desktop + server only): `Runtime::new()`, `load(path) -> LoadedExtension { path, manifest, module }` (calls `manifest()` once), `run(ext, command, args, granted, host) -> Result<String, String>`; the `Host` trait (`list_sources`, `source(id)`) is what the guest's `call` reaches — the runtime runs `Query` / `FetchText` against the returned `Source` — implemented over the source registry by desktop and `api`. Modules are instantiated per call, so every run starts from a clean state. `discover(config_dir, folder)` lists `<config>/extensions/*.wasm` and `<folder>/.moonkale/extensions/*.wasm`.
- Guest contract: exports `alloc(len) -> ptr`, `manifest() -> packed`, `run(ptr, len) -> packed`; imports module `"moonkale"`: `log(ptr, len)`, `call(ptr, len) -> packed`. `packed` is `(ptr << 32) | len` of a UTF-8 JSON string in guest memory. Memory limits: one linear memory, fuel not metered yet.
- **Permissions are enforced per host call**: `run` receives the granted set (from `Settings.extensions.permissions[id]`); a call whose permission is missing gets `HostReply::err("permission … not granted")`, never a trap. The manifest's `permissions` is what the Settings panel offers to tick; it is never trusted as granted.
- `tests/wordcount.rs` builds `extensions/wordcount` for `wasm32-unknown-unknown` on demand and checks counts, the permission refusal and an unknown command.

Where it runs: desktop loads modules in-process (`desktop/src/main.rs` `wasm_ext`), the web server exposes `list_wasm_extensions(folder)` / `run_wasm_command(ext, command, args, granted)` (`api/src/wasm.rs`); the browser never compiles wasmtime. The agent turns every `llm_tool` command into a tool (`editors/agent`), classified `Mutating` by policy (unknown tools are never read-only).

Next (documented, not started): WIT/component model, a browser runtime (P-28), fuel/epoch limits, hot reload of a module during development.

## Milestone 8: the browser runtime
The same ABI runs in the page: `packages/js/wasm-host` (built into `ui/assets/wasm_host.js`) creates a Worker per run, instantiates the module with `moonkale.log`/`moonkale.call` imports, and serves the *synchronous* `call` by posting the request to the main thread and blocking on `Atomics.wait` over an 8 MB `SharedArrayBuffer` mailbox until the reply is written back. The main thread's answer is Rust (`Workspace::answer_host_call`: the same three calls and permission check as `handle_call` here, over the client's remote sources). Requirements: a cross-origin-isolated page (the server sends COOP/COEP) and `/api/ext/module/{id}` for the bytes. When the page is not isolated, the client falls back to the server runtime. This closes P-28 for the JSON ABI; a WIT/component runtime would sit on the same Worker.
