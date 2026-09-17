//! Layouts. `trait Layout { fn step(&mut self, scene, dt) -> Converged }`
//! so all layouts are incremental/animated.
//!
//! - [`force_gpu`]: Barnes-Hut or grid-binned repulsion + spring attraction
//!   as wgpu compute shaders. This is what keeps 100k nodes fluid.
//! - [`force_cpu`]: fallback (WebGL2 has no compute) — `fdg-sim` or own
//!   implementation, degraded quality above ~10k nodes.
//! - [`hierarchical`]: layered (Sugiyama-style) for DAGs: call graphs, ASTs,
//!   ML model graphs, stack traces.
//! - [`radial`] / [`concentric`]: for "local graph around a node".
//! - Positions are persisted per view so a graph looks the same tomorrow.

pub mod force_cpu;
pub mod force_gpu;
pub mod hierarchical;
pub mod radial;
