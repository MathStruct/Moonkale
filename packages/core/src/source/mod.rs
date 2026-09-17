//! The `Source` abstraction — how the outside world becomes graph.
//!
//! A source is anything that can answer "what nodes/edges do you have?" and
//! (optionally) accept changes: a folder, a SQL database, a graph database, a
//! remote Moonkale server. `core` defines only the **trait and the
//! query/result types**; implementations live in `moonkale-project-fs`,
//! `moonkale-sources-*` (native only) and `api::RemoteSource` (the proxy the
//! web build uses).
//!
//! Object safety: the trait is `async` via `async-trait` so it can be held as
//! `Arc<dyn Source>`. On wasm32 futures are `!Send`, so the `?Send` variant is
//! selected there; on native the futures are `Send` so the server can spawn
//! them onto tokio.

pub mod descriptor;
pub mod event;
pub mod query;
pub mod transaction;

pub use descriptor::{Capabilities, SourceDescriptor, SourceFamily};
pub use query::{Query, QueryResult};
pub use transaction::{Applied, Op, Splice, TextPatch, Transaction};

use crate::error::SourceError;
use crate::graph::Version;
use crate::id::{NodeId, SourceId};

/// Re-exported so implementors don't need their own `async-trait` dependency.
/// Implement the trait with the same two attributes `Source` itself uses:
///
/// ```ignore
/// #[cfg_attr(not(target_arch = "wasm32"), moonkale_core::async_trait)]
/// #[cfg_attr(target_arch = "wasm32", moonkale_core::async_trait(?Send))]
/// impl Source for MySource { ... }
/// ```
pub use async_trait::async_trait;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Source: MaybeSendSync {
    fn id(&self) -> SourceId;
    fn descriptor(&self) -> SourceDescriptor;

    /// Structured query. Every source supports the structured IR; text
    /// queries come later.
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError>;

    /// The text body of a node plus the version it was read at.
    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError>;

    /// Apply a transaction. Partial success is reported per op in `Applied`.
    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError>;
}

/// `Send + Sync` on native (sources are shared across tokio tasks), nothing
/// on wasm32 (single-threaded, and JS handles are `!Send`).
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> MaybeSendSync for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSendSync for T {}
