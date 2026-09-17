//! Change notifications flowing *from* a source.
//!
//! `SourceEvent::{NodeChanged, NodeRemoved, EdgeChanged, EdgeRemoved,
//! SchemaChanged, Disconnected, Reconnected}` — coarse-grained on purpose.
//! A folder source emits these from a file watcher; a Postgres source may
//! emit them from `LISTEN/NOTIFY`; many sources emit nothing but
//! `Disconnected` and rely on polling by the index layer.
//!
//! Events carry ids and versions, not content; consumers re-`fetch` what they
//! care about. This keeps the event bus cheap and makes it safe to fan out to
//! many editors and to the index.
