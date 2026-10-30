//! The `Source` abstraction — how the outside world becomes graph.
//!
//! A source is anything that can answer "what nodes/edges do you have?" and
//! (optionally) accept changes: a folder, a SQL database, a graph database, a
//! remote Moonkale server. `core` defines only the **trait and the
//! query/result types**; implementations live in `moonkale-project-fs`,
//! `moonkale-sources-*` (native only) and `moonkale_server::RemoteSource` (the proxy the
//! web build uses).
//!
//! Object safety: the trait is `async` via `async-trait` so it can be held as
//! `Arc<dyn Source>`. On wasm32 futures are `!Send`, so the `?Send` variant is
//! selected there; on native the futures are `Send` so the server can spawn
//! them onto tokio.

pub mod coarse;
pub mod descriptor;
pub mod event;
pub mod opener;
pub mod query;
pub mod risk;
pub mod rows;
pub use rows::{RowBatch, RowChange, RowKey, RowOp, RowPage, RowSnapshot};
pub mod transaction;

pub use descriptor::{Capabilities, SourceDescriptor, SourceFamily, SourceTruth, TextDialect};
pub use event::Changes;
pub use opener::{Openers, Shape, SourceOpener};
pub use query::{Direction, Query, QueryResult, Table};
pub use risk::Risk;
pub use transaction::{Applied, Op, OpResult, Splice, TextPatch, Transaction};

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

    /// The text body plus its exact content version. Use this version for
    /// `WriteText.expected`; a query node may carry only a metadata hint.
    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError>;

    /// The raw bytes of a node (images and other blobs, spec 008). Default:
    /// unsupported — text-only sources need not implement it. The returned
    /// content version may differ from a query node's metadata hint.
    async fn fetch_bytes(&self, node: NodeId) -> Result<(Vec<u8>, Version), SourceError> {
        let _ = node;
        Err(SourceError::Unsupported("binary content".into()))
    }

    /// Read a bounded page with authoritative keys and exact row versions.
    async fn read_rows(&self, _table: String, _limit: usize) -> Result<RowPage, SourceError> {
        Err(SourceError::Unsupported("row reads".into()))
    }

    /// Check a typed row proposal without persisting it. Apply rechecks versions.
    async fn preview_rows(&self, _batch: RowBatch) -> Result<(), SourceError> {
        Err(SourceError::Unsupported("row writes".into()))
    }

    /// Apply a typed row batch in one database transaction or refuse it entirely.
    /// Returned entries correspond to operations; deletes return None.
    async fn apply_rows(&self, _batch: RowBatch) -> Result<Vec<Option<RowSnapshot>>, SourceError> {
        Err(SourceError::Unsupported("row writes".into()))
    }

    /// Preview a bounded text proposal without changing authored state.
    /// A successful preview does not reserve versions: apply must check again.
    /// Store-backed implementations can validate their transaction's constraints.
    async fn preview_text(&self, tx: Transaction) -> Result<(), SourceError> {
        tx.validate_text_proposal()?;
        if !self.descriptor().capabilities.write {
            return Err(SourceError::Unsupported("read-only source".into()));
        }
        if tx.ops.is_empty() || tx.ops.len() > 64 {
            return Err(SourceError::Invalid(
                "text proposal requires 1..64 writes".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0usize;
        for op in tx.ops {
            let Op::WriteText {
                node,
                expected,
                patch,
            } = op
            else {
                return Err(SourceError::Unsupported("text proposals only".into()));
            };
            if !seen.insert(node) {
                return Err(SourceError::Invalid("duplicate proposal node".into()));
            }
            let (text, actual) = self.fetch_text(node).await?;
            if expected != actual {
                return Err(SourceError::Conflict { expected, actual });
            }
            let next = patch.apply(&text)?;
            bytes = bytes.saturating_add(text.len()).saturating_add(next.len());
            if text.len() > 512 * 1024 || next.len() > 512 * 1024 || bytes > 4 * 1024 * 1024 {
                return Err(SourceError::Invalid(
                    "text proposal exceeds byte budget".into(),
                ));
            }
        }
        Ok(())
    }

    /// Apply a transaction. Partial success is reported per op in `Applied`.
    /// When `capabilities.atomic_text` is true, a transaction containing only
    /// WriteText operations must instead be all-or-nothing, with every expected
    /// version validated inside the same source transaction.
    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError>;

    /// A node changed (it was saved through *another* source, or externally):
    /// re-derive whatever this source keeps about it. Default: nothing.
    /// The index re-extracts the file; a folder has nothing to do.
    async fn refresh(&self, _node: NodeId) -> Result<(), SourceError> {
        Ok(())
    }

    /// One upstream debounce window. Derived sources can commit all changed
    /// nodes atomically (including both sides of a rename). The default keeps
    /// existing sources compatible by refreshing each node in order.
    async fn refresh_batch(&self, nodes: &[NodeId]) -> Result<(), SourceError> {
        for node in nodes {
            self.refresh(*node).await?;
        }
        Ok(())
    }

    /// Changes made to the source from outside Moonkale after log position
    /// `since` (see [`Changes`]). A long poll: returns as soon as there is
    /// something newer, or after a bounded wait with an empty `paths`.
    /// `Ok(None)`: this source is not watched — the UI offers a refresh
    /// button instead (Milestone 16). Pass `0` the first time; the answer's
    /// `seq` is the position to continue from.
    async fn changes_since(&self, _since: u64) -> Result<Option<Changes>, SourceError> {
        Ok(None)
    }

    /// What running `text` in `dialect` would do — the read-only gate and
    /// the agent's policy both ask this. Default: the shared rules in
    /// [`risk::classify`]; a source that knows better (functions that read
    /// files, a dialect of its own) overrides it.
    fn classify(&self, dialect: &str, text: &str) -> Risk {
        risk::classify(dialect, text)
    }
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
