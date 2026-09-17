---
tags: [research, extensions]
---
# WASM Extension Runtimes

Decision: [[ADR-0004 WASM components for extensions]].

| Option | Sandbox | Portability | Languages | Notes |
|---|---|---|---|---|
| Native dylib (`libloading`) | none | per-OS builds | Rust | fastest; unsafe; no web. Rejected. |
| Scripting (Lua `mlua`, JS `boa`/`quickjs`) | partial | good | one scripting lang | second language for authors; JS engine in the app is what we're avoiding. Rejected. |
| WASM core modules (`extism` 1.30) | ✅ | ✅ | many | simple ABI (bytes in/out); no typed interfaces; plugin host on web via `extism-js`. Runner-up. |
| **WASM components (WIT)** — `wasmtime` 49 native, `wasm_component_layer` 0.1 on web | ✅ | ✅ | many | typed interfaces generated from `ext-api`; WASI p2 for scoped fs; browser side least mature. **Chosen.** |

## Browser hosting notes
The app is wasm; it can't embed wasmtime. `wasm_component_layer` implements the component model over a pluggable backend (JS `WebAssembly` in the browser). Run in a Worker for isolation; messages instead of calls. Alternative: transpile components to core modules + JS glue at publish time (`jco`) — pushes work to the publisher, simplifies the client. Evaluate both; ranked very high difficulty in [[Problem Ranking]].

## Mobile
No runtime code loading in v1 (store policy risk on iOS; unverified for wasm). Static only.
