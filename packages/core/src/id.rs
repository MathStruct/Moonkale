//! Stable identifiers.
//!
//! Every entity in Moonkale has a globally unique, *stable* id. Stability
//! matters more than it first looks: layouts are persisted by id, extensions
//! refer to nodes by id, LLM tool results cite ids, and a graph database row
//! may be re-read a thousand times and must map to the same node each time.
//!
//! Ids are opaque to editors. A graph editor must not parse a `NodeId` to
//! discover "this is a file"; that information lives in `Node::kind`.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Identifies one opened source: one folder, one database connection.
///
/// Human-readable on purpose (`"folder:/home/me/proj"`) so logs and URLs make
/// sense; uniqueness is the registry's job.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifies one node.
///
/// For entities that originate in a source (a file path, a table row, a graph
/// vertex) the id is *derived* deterministically from `(SourceId, native key)`
/// with UUID v5, so re-opening the same source yields the same ids without a
/// lookup table. Locally created entities use [`NodeId::fresh`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

/// Namespace for derived ids. Changing it invalidates every persisted id.
const NODE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6d, 0x6f, 0x6f, 0x6e, 0x6b, 0x61, 0x6c, 0x65, // "moonkale"
    0x2d, 0x6e, 0x6f, 0x64, 0x65, 0x2d, 0x30, 0x31, // "-node-01"
]);

impl NodeId {
    /// Deterministic id for an entity a source owns, keyed by its native
    /// identity (a relative path, a primary key, a vertex id).
    pub fn derive(source: &SourceId, native_key: &str) -> Self {
        let mut name = Vec::with_capacity(source.0.len() + 1 + native_key.len());
        name.extend_from_slice(source.0.as_bytes());
        name.push(0);
        name.extend_from_slice(native_key.as_bytes());
        Self(Uuid::new_v5(&NODE_NAMESPACE, &name))
    }

    /// A fresh id for something created in the app that no source owns yet.
    /// UUID v5 over a caller-provided unique string keeps `core` free of a
    /// random-number dependency; callers pass e.g. a timestamp + counter.
    pub fn fresh(unique: &str) -> Self {
        Self::derive(&SourceId::new("local"), unique)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::str::FromStr for NodeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_ids_are_deterministic() {
        let src = SourceId::new("folder:/tmp/p");
        assert_eq!(
            NodeId::derive(&src, "src/main.rs"),
            NodeId::derive(&src, "src/main.rs")
        );
        assert_ne!(
            NodeId::derive(&src, "src/main.rs"),
            NodeId::derive(&src, "src/lib.rs")
        );
    }

    #[test]
    fn derived_ids_are_scoped_by_source() {
        let a = SourceId::new("a");
        let b = SourceId::new("b");
        assert_ne!(NodeId::derive(&a, "x"), NodeId::derive(&b, "x"));
    }

    #[test]
    fn separator_prevents_collisions() {
        // "ab" + "c" must not equal "a" + "bc"
        assert_ne!(
            NodeId::derive(&SourceId::new("ab"), "c"),
            NodeId::derive(&SourceId::new("a"), "bc")
        );
    }

    #[test]
    fn display_round_trips() {
        let id = NodeId::derive(&SourceId::new("s"), "k");
        assert_eq!(id.to_string().parse::<NodeId>().unwrap(), id);
    }
}
