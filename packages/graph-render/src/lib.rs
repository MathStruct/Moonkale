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
//! are unit-tested natively; only `web` touches the browser. [`scene`] is
//! the Rust API around them — graph in, events out — for hosts other than
//! the page (Milestone 18 phase 6.1; see `README.md` and
//! `examples/headless.rs`). The crate depends on no Moonkale crate.
//!
//! Build: `packages/graph-render/build.sh` → `packages/editors/graph/assets/`.

pub mod camera;
pub mod frame;
pub mod graph;
pub mod layout;
pub mod quadtree;
pub mod scene;

#[cfg(target_arch = "wasm32")]
pub mod render;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(test)]
mod shader_tests {
    /// `shaders.wgsl` is only compiled by the GPU at runtime — a syntax
    /// error parks every graph panel behind a wgpu validation panic (spec
    /// 031, stage 1: it happened twice in one evening). Validate it with
    /// naga, the same front end wgpu uses, in the native test run instead.
    #[test]
    fn shaders_validate() {
        let src = include_str!("shaders.wgsl");
        let module = naga::front::wgsl::parse_str(src).expect("wgsl parses");
        for name in [
            "node_vs", "node_fs", "edge_vs", "edge_fs", "arrow_vs", "arrow_fs",
        ] {
            assert!(
                module.entry_points.iter().any(|ep| ep.name == name),
                "missing entry point {name}"
            );
        }
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("wgsl validates");
    }
}
