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
}

impl Edge {
    pub fn contains(source: &SourceId, parent: NodeId, child: NodeId) -> Self {
        Self {
            source: source.clone(),
            from: parent,
            to: child,
            kind: EdgeKind::Contains,
        }
    }
}
