//! `Node` — the unit of everything.

use crate::id::{NodeId, SourceId};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Monotonic-per-node change marker used for optimistic concurrency and undo.
/// Sources decide how to compute it (a content hash, an mtime, a row version);
/// consumers only compare for equality.
///
/// Serialized as a hex **string**, not a number: versions are 64-bit hashes
/// and JavaScript numbers lose precision above 2⁵³, which would corrupt them
/// on any JSON path through the browser (`BroadcastChannel`, `eval`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Version(pub u64);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{:x}", self.0)
    }
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{:x}", self.0))
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        u64::from_str_radix(&text, 16)
            .map(Version)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod version_tests {
    use super::Version;

    #[test]
    fn version_survives_json_as_a_string() {
        let v = Version(u64::MAX - 12345);
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.starts_with('"'), "must be a string, got {json}");
        assert_eq!(serde_json::from_str::<Version>(&json).unwrap(), v);
    }
}

/// An *open* enum: well-known kinds editors can match on, plus `Custom` so an
/// extension can introduce "ModelingToolkit.jl component" without a core
/// change. Milestone 1 uses `Directory` and `File`; the others are listed so
/// the shape is settled.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Directory,
    File,
    Database,
    Table,
    Column,
    Row,
    Vertex,
    Key,
    Page,
    Symbol,
    Block,
    Custom(String),
}

/// How to get a node's body — not the body itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentRef {
    /// UTF-8 text; `lang` is a language id when known (`"rust"`).
    Text {
        len: u64,
        lang: Option<String>,
    },
    Blob {
        len: u64,
        mime: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// Who owns the truth for this node.
    pub source: SourceId,
    pub kind: NodeKind,
    /// What the UI shows when it has one line (a file name, a row's key).
    pub label: String,
    /// The source-native key the id was derived from (a relative path for
    /// folders). Editors use it for display and for language detection; they
    /// never re-derive ids from it.
    pub native_key: String,
    pub content: Option<ContentRef>,
    pub version: Version,
}

impl Node {
    /// Best-effort language id from the native key's extension. Lives here
    /// (not in the index) because Milestone 1 has no index yet; the index
    /// will override it with real detection later.
    pub fn language_hint(&self) -> Option<&'static str> {
        let ext = self.native_key.rsplit('.').next()?;
        Some(match ext {
            "rs" => "rust",
            "jl" => "julia",
            "go" => "go",
            "u" => "unison",
            "lean" => "lean",
            "md" => "markdown",
            "typ" => "typst",
            "toml" => "toml",
            "json" => "json",
            "ts" | "mts" => "typescript",
            "js" | "mjs" => "javascript",
            "css" => "css",
            "html" => "html",
            "py" => "python",
            "sql" => "sql",
            "yml" | "yaml" => "yaml",
            _ => return None,
        })
    }
}
