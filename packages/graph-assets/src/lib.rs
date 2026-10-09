//! The graph engine's wasm module and its JS glue, as build assets — one
//! copy shared by every host panel (the graph view and the flow editor,
//! ADR-0018). `packages/graph-render/build.sh` writes them here; the crate
//! exists so no extension depends on another (the layering check,
//! Milestone 18): editors and extensions depend on this one.

use dioxus::prelude::*;

/// The JS module (`wasm-bindgen` glue) — import it from the host's script.
pub const RENDER_JS: Asset = asset!("/assets/graph_render.js");

/// The engine itself, compiled by `packages/graph-render/build.sh`.
pub const RENDER_WASM: Asset = asset!("/assets/graph_render_bg.wasm");
