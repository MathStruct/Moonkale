//! # moonkale-editor-graph
//!
//! The graph "window" and the reason this crate is the biggest bet after
//! the model: **it must stay fluid at 100k+ nodes**, where DOM/SVG/Canvas-2D
//! approaches (Obsidian, Cytoscape) degrade.
//!
//! Architecture:
//!
//! ```text
//!   GraphView (core) ─► scene (instance buffers) ─► wgpu render passes
//!                          ▲                          ├─ nodes (instanced quads/SDF glyphs)
//!   layout (compute) ──────┘                          ├─ edges (instanced lines/arcs, per-edge colour+direction)
//!                                                     ├─ labels (glyph atlas, LOD)
//!                                                     └─ picking (id buffer)
//! ```
//!
//! `wgpu` gives one code path for WebGPU (web), Vulkan/Metal/DX12 (desktop),
//! and Metal/Vulkan (mobile). The hard part is not rendering; it is *where the
//! surface comes from* on each platform — see [`render::surface`] and the
//! long discussion in the vault (`editors/Graph View.md`,
//! `decisions/ADR-0003 wgpu for graph rendering.md`).
//!
//! Features by module: [`layout`] (force-directed on GPU compute, plus
//! hierarchical/radial CPU layouts), [`interact`] (pan/zoom/select/drag,
//! hover popup, keyboard), [`style`] (declarative node/edge styles from
//! renderer contributions), [`subgraph`] (expand/collapse, filters, saved
//! views), [`three_d`] (3D mode: same buffers, a perspective camera and
//! depth-sorted edges).

pub mod interact;
pub mod layout;
pub mod panel;
pub mod render;
pub mod scene;
pub mod style;
pub mod subgraph;
pub mod three_d;
