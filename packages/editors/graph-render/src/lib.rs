//! # moonkale-graph-render
//!
//! The graph view's renderer, compiled to a **standalone wasm module** and
//! loaded into the page (web) or the webview (desktop) like the CodeMirror
//! bundle — see vault `decisions/ADR-0011 Desktop graph surface strategy.md`
//! (plan A). It knows nothing about Moonkale's model: it receives a JSON
//! graph, lays it out, draws it with `wgpu` (WebGPU, or WebGL2 where WebGPU
//! is missing) and reports hover/click events back.
//!
//! Layout and hit-testing are plain Rust (`graph`, `layout`, `camera`) and
//! are unit-tested natively; only `web` touches the browser.
//!
//! Build: `packages/editors/graph-render/build.sh` → `../graph/assets/`.

pub mod camera;
pub mod graph;
pub mod layout;

#[cfg(target_arch = "wasm32")]
pub mod render;
#[cfg(target_arch = "wasm32")]
pub mod web;
