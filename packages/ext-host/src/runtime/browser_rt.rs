//! Browser runtime (web build).
//!
//! The app is itself wasm here, so we cannot embed wasmtime. Instead the
//! component is transpiled/loaded through `wasm_component_layer` backed by
//! the browser's `WebAssembly` API, running on a Web Worker so a misbehaving
//! extension cannot freeze the UI. Message passing replaces direct calls;
//! latency is higher, which is acceptable for panels and commands but is why
//! renderers/languages are declarative data rather than callbacks.
