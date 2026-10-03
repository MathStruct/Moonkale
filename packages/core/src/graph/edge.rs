//! `Edge` — a typed, directed relation between nodes.
//!
//! Direction is semantic (`from` contains `to`); the graph view is free to
//! draw it any way it likes. An edge can span two sources; the source that
//! *stores* it is its owner.

use crate::id::{NodeId, SourceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Contains,
    References,
    Links,
    Calls,
    ForeignKey,
    Defines,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub source: SourceId,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    /// Typed properties (Milestone 18 phase 6.2): a weight, a column of a
    /// foreign key, the evidence on an equivalence edge (Sophia).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub props: crate::graph::Properties,
}

impl Edge {
    pub fn contains(source: &SourceId, parent: NodeId, child: NodeId) -> Self {
        Self {
            props: Default::default(),
            source: source.clone(),
            from: parent,
            to: child,
            kind: EdgeKind::Contains,
        }
    }
}
