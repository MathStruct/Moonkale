//! # moonkale-core
//!
//! The platform-agnostic heart of Moonkale. Everything here compiles on
//! desktop, web (wasm32) and mobile, and depends on **nothing** that does I/O.
//!
//! Moonkale is *graph-native*: whatever the user opens — a folder, a Postgres
//! schema, a TypeDB database, a Redis keyspace — is presented to the rest of
//! the app as a graph of [`graph::Node`]s connected by [`graph::Edge`]s. Editors
//! never talk to a file system or a database directly; they talk to this model,
//! and [`source::Source`]s translate between the model and the outside world.
//!
//! Layering (arrows = "may depend on"):
//!
//! ```text
//!   editors/*  ──►  ui (shell)  ──►  ext-api  ──►  core
//!   sources-* ──►  sources      ──►  core
//!   index, llm, lsp, terminal   ──►  core
//! ```
//!
//! `core` is at the bottom and depends on nothing in the workspace. If you find
//! yourself wanting to add `dioxus` or `tokio` here: stop, and put it one layer
//! up. The reason is that `core` is what extensions compiled to WASM see, and it
//! must stay tiny, stable and free of host assumptions.
//!
//! **Milestone 1 status**: `id`, `graph::{node,edge}`, `source::{query,
//! transaction, descriptor}` and the `Source` trait are implemented at the
//! size the walking skeleton needs. `graph::{property,view}`, `command` and
//! `source::event` are still design stubs. See `core.md` next to this crate.

pub mod command;
pub mod error;
pub mod graph;
pub mod id;
pub mod source;

pub use error::SourceError;
pub use graph::{ContentRef, Edge, EdgeKind, Node, NodeKind, Version};
pub use id::{NodeId, SourceId};
pub use source::async_trait;
pub use source::{
    Applied, Capabilities, Op, Query, QueryResult, Source, SourceDescriptor, SourceFamily, Splice,
    TextPatch, Transaction,
};
