//! Change notifications flowing *from* a source (design stub — Milestone 1
//! has no watching).
//!
//! Planned: `SourceEvent::{NodeChanged, NodeRemoved, EdgeChanged,
//! EdgeRemoved, SchemaChanged, Disconnected, Reconnected}` carrying ids and
//! versions, not content; consumers re-`fetch` what they care about.
