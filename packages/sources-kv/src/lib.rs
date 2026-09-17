//! # moonkale-sources-kv
//!
//! Redis and Dragonfly speak the same protocol, so this is one source with a
//! `flavor` field. KV stores are flat, which is the challenge: the
//! [`patterns`] module turns `user:42:profile` into a tree of synthetic nodes
//! (`user` → `42` → `profile`) so the explorer and graph view stay usable.
//! Value types (string/hash/list/set/zset/stream/JSON) become `ContentRef`s.
//!
//! Keyspace notifications give real `SourceEvent`s when enabled on the
//! server; otherwise `SCAN`-based polling.

pub mod patterns;
#[cfg(feature = "redis")]
pub mod redis;
pub mod values;
