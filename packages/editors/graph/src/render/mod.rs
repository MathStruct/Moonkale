//! wgpu rendering.
//!
//! Passes: `edges` (instanced quads along segments; colour per edge kind or
//! per-edge override; arrowheads for direction; dashed/bidirectional
//! variants), `nodes` (instanced SDF circles/rects/icons), `labels`
//! (glyph atlas, hidden below a zoom threshold), `pick` (render ids to an
//! offscreen buffer; read back on hover/click). LOD: beyond N nodes,
//! labels off and edges thinned by importance.

pub mod passes;
pub mod pick;
pub mod surface;
pub mod text;
