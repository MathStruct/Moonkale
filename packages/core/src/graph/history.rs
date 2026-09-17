//! The entity log (design stub — see vault `architecture/Version Management.md`
//! and `decisions/ADR-0012 Two histories.md`).
//!
//! Unlike git, which tracks *diffs of text keyed by path*, the graph's
//! history is an append-only log of events on UUID-identified entities:
//!
//! ```ignore
//! pub struct Event {
//!     pub id: EventId,          // UUID v7: time-ordered
//!     pub recorded_at: Timestamp,
//!     pub valid_at: Option<Timestamp>,   // for imported facts (bitemporal)
//!     pub actor: ActorId,       // user | agent | extension | importer
//!     pub kind: EventKind,      // Add(Entity) | Remove(EntityId)
//!                               // | SetProps(EntityId, PropDelta) | Content(NodeId, PatchId)
//!     pub cause: Option<EventId>,   // undo / merge / import provenance
//!     pub tx: TransactionId,
//! }
//! ```
//!
//! State is a fold over the log; deletions are tombstones; snapshots are an
//! optimisation; `Version` becomes "the last event that touched this node".
//! Add/Remove of distinct ids commute, so structural changes from two
//! replicas merge by set semantics; only *content* needs a text patch / CRDT.
//! Not implemented in Milestone 1.
