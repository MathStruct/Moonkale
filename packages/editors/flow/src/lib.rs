//! # moonkale-editor-flow
//!
//! A flow is a graph with a *schema*: block kinds with typed input/output
//! ports, and wires that must respect port types
//! (`moonkale_ext_api::flow`). It is stored as `*.flow.json` in the folder
//! and edited here on a [dioxus-flow](https://crates.io/crates/dioxus-flow)
//! canvas with a palette, typed wiring (a mismatch is refused), per-block
//! parameters, validation and **codegen** through whichever library the
//! blocks came from.
//!
//! Block libraries are extension contributions
//! (`Extension::flow_libraries`); this crate ships none. The Lux.jl library
//! is the optional `moonkale-ext-lux` extension — off by default.
//!
//! Vault: `editors/Flow Editor.md`, `research/dioxus-flow.md`.

mod extension;
mod panel;

pub use extension::{is_flow, FlowExtension, EDITOR_PREFIX};
