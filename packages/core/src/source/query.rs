//! The structured query IR.
//!
//! Two levels are planned: this structured `Query` every source must support,
//! and `TextQuery { dialect, text }` for raw SQL/Cypher (later). Milestone 1
//! needs exactly two shapes.

use crate::graph::{Edge, Node};
use crate::id::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Query {
    /// One node by id.
    Node(NodeId),
    /// Direct children of a node (the `Contains` neighbourhood, depth 1).
    Children(NodeId),
}

/// A materialised subgraph. Always finite; sources paginate internally when
/// a neighbourhood is huge (not needed for Milestone 1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl QueryResult {
    pub fn single(node: Node) -> Self {
        Self {
            nodes: vec![node],
            edges: Vec::new(),
        }
    }
}
