//! wasmtime runtime (desktop + server).
//!
//! Loads a WASM *component* implementing the `moonkale:ext` WIT world.
//! Each extension gets its own `Store` with fuel/epoch limits, a WASI
//! preview-2 context restricted to its own storage directory, and host
//! functions that forward to `Host`. Panels render through the `ui::Tree`
//! protocol (see `ext-api::host`).
//!
//! Open question logged in the vault: hot-reload of components during
//! extension development (watch the `.wasm`, re-instantiate, replay state).
