//! `SourceDescriptor` — the static shape of a source.
//!
//! Connection *secrets* are NOT in the descriptor.

use crate::id::{NodeId, SourceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceFamily {
    Folder,
    Sql,
    Graph,
    KeyValue,
    /// Derived data over other sources (links, symbols).
    Index,
    Remote,
    Custom(String),
}

/// What a source can do. Editors and the LLM policy feature-detect against
/// this instead of matching on the brand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub read: bool,
    pub write: bool,
    pub watch: bool,
    /// Dialect accepted by `Query::Text`, if any (`"sql"`).
    pub text_query: Option<TextDialect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextDialect {
    Sql,
    Cypher,
    TypeQl,
    /// Key/value stores (Milestone 17): `scan <table> [prefix <p>] [limit <n>]`,
    /// `get <table> <key>`.
    Kv,
    /// HelixDB (Milestone 17): `nodes [<label>] [limit <n>]`,
    /// `edges [<label>] [limit <n>]`.
    Helix,
}

impl TextDialect {
    /// The name `Query::Text { dialect }` carries for this dialect.
    pub fn name(self) -> &'static str {
        match self {
            TextDialect::Sql => "sql",
            TextDialect::Cypher => "cypher",
            TextDialect::TypeQl => "typeql",
            TextDialect::Kv => "kv",
            TextDialect::Helix => "helix",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDescriptor {
    pub id: SourceId,
    pub display_name: String,
    pub family: SourceFamily,
    pub capabilities: Capabilities,
    /// The node to start browsing from (the folder itself, the schema root).
    pub root: NodeId,
}
