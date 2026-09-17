//! `Node` — the unit of everything.

use crate::id::{NodeId, SourceId};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Monotonic-per-node change marker used for optimistic concurrency and undo.
/// Sources decide how to compute it (a content hash, an mtime, a row version);
/// consumers only compare for equality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version(pub u64);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{:x}", self.0)
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
    Table,
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
