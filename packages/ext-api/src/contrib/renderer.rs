//! `RendererContribution` — custom node/edge visuals in the graph and flow
//! views.
//!
//! A renderer says "for nodes of kind K, draw with this glyph / colour /
//! shape / popup template". Two tiers:
//! - **declarative** (`NodeStyle`, `EdgeStyle`, a popup `ui::Tree` template):
//!   works everywhere, including wasm extensions, and is GPU-batched.
//! - **custom** (a Dioxus component for the popup, or a `wgpu` render pass):
//!   static extensions only.
