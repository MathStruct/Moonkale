//! File watching (design note — not in Milestone 1).
//!
//! Planned: `trait Watcher` with a `notify`-backed impl (native), a polling
//! impl (web FSA), and a no-op impl (mobile), debounced and coalesced into
//! `SourceEvent`s. Until then, an external change is detected only at save
//! time through the version check (`SourceError::Conflict`).
