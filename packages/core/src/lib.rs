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
//! What is here: ids, `graph::{node, edge, property, history}` (the entity
//! log), `source::{query, transaction, descriptor, event}` and the `Source`
//! trait. Commands live in `moonkale-ext-api`; `GraphView` and `when`
//! clauses are designs in the vault (`architecture/Graph-Native Model.md`,
//! `extensions/Contribution Points.md`). See `core.md` next to this crate.

pub mod error;
pub mod graph;
pub mod id;
pub mod source;

/// The version this build reports (`moonkale-server --version`, MCP's
/// `serverInfo`, the remote-server directory on SSH hosts): the release name
/// (`260927-proto`) when the release workflow set `MOONKALE_RELEASE` at build
/// time, otherwise the crate version. Releases are named by date and the
/// crate version stays `0.1.0`, so without this every release looked the same
/// — and a remote host kept the server uploaded by an older release.
pub const VERSION: &str = match option_env!("MOONKALE_RELEASE") {
    Some(v) if !v.is_empty() => v,
    _ => env!("CARGO_PKG_VERSION"),
};

pub use error::SourceError;
pub use graph::{Actor, EntityLog, Event, EventId, EventKind, HistoryState};
pub use graph::{ContentRef, Edge, EdgeKind, Node, NodeKind, Value, Version};
pub use id::{NodeId, SourceId};
pub use source::async_trait;
pub use source::{
    Applied, Capabilities, Changes, Direction, Op, OpResult, Openers, Query, QueryResult, Risk,
    Shape, Source, SourceDescriptor, SourceFamily, SourceOpener, Splice, Table, TextDialect,
    TextPatch, Transaction,
};
