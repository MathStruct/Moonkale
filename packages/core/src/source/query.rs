//! The query model.
//!
//! Two levels: the structured [`Query`] every source must support (listing,
//! neighbourhoods, "everything"), and [`Query::Text`] — raw SQL / Cypher /
//! TypeQL passed through to sources whose capabilities allow it. Results are
//! a subgraph plus, for text queries, an optional [`Table`] of rows.

use crate::graph::{Edge, Node, NodeKind, Value};
use crate::id::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Out,
    In,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Query {
    /// One node by id.
    Node(NodeId),
    /// Direct children of a node (the `Contains`/`Defines` neighbourhood, depth 1).
    Children(NodeId),
    /// Nodes reachable from `node` within `depth` hops along edges of the
    /// given direction, plus the edges among them. `depth = 1` is the
    /// "local graph" of a page.
    Neighbours {
        node: NodeId,
        depth: u32,
        direction: Direction,
    },
    /// Everything the source has, capped. For sources that can answer it
    /// cheaply (the index, small databases); folders answer with their
    /// listed tree.
    All {
        limit: usize,
        kinds: Option<Vec<NodeKind>>,
    },
    /// A query in the source's own language. `dialect` is `"sql"`,
    /// `"cypher"`, … — the source rejects dialects it doesn't speak.
    Text { dialect: String, text: String },
}

/// Rows from a text query (or a table listing).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// `true` when the source cut the result at its row cap.
    pub truncated: bool,
}

/// A materialised subgraph. Always finite; sources paginate or cap internally.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Present for `Query::Text` (and table listings).
    pub table: Option<Table>,
    /// `true` when a cap (`All { limit }`, a row cap) cut the result.
    pub truncated: bool,
}

impl QueryResult {
    pub fn single(node: Node) -> Self {
        Self {
            nodes: vec![node],
            ..Default::default()
        }
    }
}
