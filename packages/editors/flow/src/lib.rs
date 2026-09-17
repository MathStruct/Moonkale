//! # moonkale-editor-flow
//!
//! A flow is a graph with a *schema*: block kinds with typed input/output
//! ports, and edges that must respect port types. It is stored as ordinary
//! nodes/edges (`NodeKind::Block`, `EdgeKind::Custom("flow.wire")`) so it
//! lives in any source and shows up in the graph view — but it is *edited*
//! here with snapping, port validation and a palette.
//!
//! Block libraries are extension contributions (`FlowLibrary { blocks,
//! codegen }`). First target: **Lux.jl layers** (Dense, Conv, Chain, …) with
//! codegen to a Julia file, then ModelingToolkit.jl components. Codegen is
//! itself a command, so an LLM can be asked to "wire a CNN" and the result
//! is just a transaction.
//!
//! Rendering: DOM/SVG (Dioxus) for typical flows (< a few hundred blocks)
//! because blocks need rich, editable content; the graph renderer's surface
//! for very large flows is a later optimisation.

pub mod canvas;
pub mod codegen;
pub mod palette;
pub mod panel;
pub mod schema;
pub mod validate;
