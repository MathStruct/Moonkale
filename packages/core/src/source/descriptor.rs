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

/// Where authored content lives (ADR-0028). This describes ownership, not
/// writability: callers must still check source capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceTruth {
    /// The source's existing external representation remains authoritative.
    #[default]
    Files,
    /// Content is authored directly in a graph-native store.
    Store,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDescriptor {
    /// Defaults to external/file truth when reading pre-ADR-0028 descriptors.
    #[serde(default)]
    pub truth: SourceTruth,
    pub id: SourceId,
    pub display_name: String,
    pub family: SourceFamily,
    pub capabilities: Capabilities,
    /// The node to start browsing from (the folder itself, the schema root).
    pub root: NodeId,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor() -> SourceDescriptor {
        let id = SourceId::new("test:truth");
        SourceDescriptor {
            truth: SourceTruth::Files,
            root: NodeId::derive(&id, "root"),
            id,
            display_name: "truth".into(),
            family: SourceFamily::Folder,
            capabilities: Capabilities::default(),
        }
    }
    #[test]
    fn old_descriptors_default_to_files_and_new_values_round_trip() {
        let mut value = serde_json::to_value(descriptor()).unwrap();
        value.as_object_mut().unwrap().remove("truth");
        let old: SourceDescriptor = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(old.truth, SourceTruth::Files);
        for (truth, wire) in [(SourceTruth::Files, "files"), (SourceTruth::Store, "store")] {
            let mut d = descriptor();
            d.truth = truth;
            let json = serde_json::to_value(&d).unwrap();
            assert_eq!(json["truth"], wire);
            assert_eq!(serde_json::from_value::<SourceDescriptor>(json).unwrap(), d);
        }
        value["truth"] = serde_json::json!("unknown");
        assert!(serde_json::from_value::<SourceDescriptor>(value).is_err());
    }
}
