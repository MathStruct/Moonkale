//! Writes.
//!
//! All mutations are a `Transaction`: ordered `Op`s, each carrying the
//! `Version` the caller last saw so the source can detect conflicts. Text
//! edits travel as *patches* (see ADR-0009): Milestone 1's editor produces a
//! single whole-document splice, but the type is right and the source
//! applies arbitrary splice lists.

use crate::error::SourceError;
use crate::graph::Version;
use crate::id::NodeId;
use serde::{Deserialize, Serialize};

/// Replace `text[start..end]` (in **chars**, not bytes) with `text`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Splice {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextPatch {
    /// Splices are applied in order, each against the text as left by the
    /// previous one.
    Replace(Vec<Splice>),
}

impl TextPatch {
    /// A patch that replaces the whole document.
    pub fn whole(new_text: impl Into<String>, old_char_len: usize) -> Self {
        TextPatch::Replace(vec![Splice {
            start: 0,
            end: old_char_len,
            text: new_text.into(),
        }])
    }

    /// Apply to `original`; errors if a splice is out of range.
    pub fn apply(&self, original: &str) -> Result<String, SourceError> {
        let TextPatch::Replace(splices) = self;
        let mut chars: Vec<char> = original.chars().collect();
        for s in splices {
            if s.start > s.end || s.end > chars.len() {
                return Err(SourceError::Invalid(format!(
                    "splice {}..{} out of range for length {}",
                    s.start,
                    s.end,
                    chars.len()
                )));
            }
            chars.splice(s.start..s.end, s.text.chars());
        }
        Ok(chars.into_iter().collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    WriteText {
        node: NodeId,
        expected: Version,
        patch: TextPatch,
    },
    /// Create a new text node under `parent` (a directory), named `name`
    /// (a relative path is allowed: missing directories are created).
    /// Refused if it already exists. The result's `node` is the new node's id.
    CreateText {
        parent: NodeId,
        name: String,
        text: String,
    },
    /// Create a directory `name` under `parent` (Milestone 7). Refused if
    /// it exists. The result's `node` is the new directory.
    CreateDir { parent: NodeId, name: String },
    /// Rename or move `node` to the relative path `to` (within the source).
    /// Refused if `to` exists. The result's `node` is the node's *new* id
    /// (ids derive from paths).
    Rename { node: NodeId, to: String },
    /// Delete `node` (a directory with everything below it). Sources that
    /// can, keep a copy (the folder source moves it to `.moonkale/trash`).
    Delete { node: NodeId },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub ops: Vec<Op>,
}

impl Transaction {
    /// Validate the shape and inserted-text budget of a reviewable text batch
    /// before fetching bases, applying patches or parsing proposed code.
    pub fn validate_text_proposal(&self) -> Result<(), SourceError> {
        if self.ops.is_empty() || self.ops.len() > 64 {
            return Err(SourceError::Invalid(
                "text proposal requires 1..64 writes".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0usize;
        let mut edits = 0usize;
        for op in &self.ops {
            let Op::WriteText { node, patch, .. } = op else {
                return Err(SourceError::Unsupported("text proposals only".into()));
            };
            if !seen.insert(*node) {
                return Err(SourceError::Invalid("duplicate proposal node".into()));
            }
            let TextPatch::Replace(splices) = patch;
            edits = edits.saturating_add(splices.len());
            for splice in splices {
                bytes = bytes.saturating_add(splice.text.len());
            }
            if edits > 4096 || bytes > 4 * 1024 * 1024 {
                return Err(SourceError::Invalid(
                    "text proposal exceeds edit/text budget".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn create_text(parent: NodeId, name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            ops: vec![Op::CreateText {
                parent,
                name: name.into(),
                text: text.into(),
            }],
        }
    }

    pub fn create_dir(parent: NodeId, name: impl Into<String>) -> Self {
        Self {
            ops: vec![Op::CreateDir {
                parent,
                name: name.into(),
            }],
        }
    }

    pub fn rename(node: NodeId, to: impl Into<String>) -> Self {
        Self {
            ops: vec![Op::Rename {
                node,
                to: to.into(),
            }],
        }
    }

    pub fn delete(node: NodeId) -> Self {
        Self {
            ops: vec![Op::Delete { node }],
        }
    }

    pub fn write_text(node: NodeId, expected: Version, patch: TextPatch) -> Self {
        Self {
            ops: vec![Op::WriteText {
                node,
                expected,
                patch,
            }],
        }
    }
}

/// Per-op outcome. A refused op does not fail the transaction as a whole so
/// the UI can partially succeed and explain why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpResult {
    Ok {
        node: NodeId,
        version: Version,
        /// Non-fatal diagnostics from a successful write. Never inserted into text.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
    },
    Refused {
        node: NodeId,
        error: SourceError,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    pub results: Vec<OpResult>,
}

impl Applied {
    /// The new version of `node` if its op succeeded.
    pub fn version_of(&self, node: NodeId) -> Option<Version> {
        self.results.iter().find_map(|r| match r {
            OpResult::Ok {
                node: n, version, ..
            } if *n == node => Some(*version),
            _ => None,
        })
    }

    pub fn first_error(&self) -> Option<&SourceError> {
        self.results.iter().find_map(|r| match r {
            OpResult::Refused { error, .. } => Some(error),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_success_results_default_to_no_warnings() {
        let node = NodeId::derive(&crate::SourceId::new("test"), "name");
        let value = serde_json::json!({"Ok": {"node": node, "version": Version(1)}});
        let OpResult::Ok { warnings, .. } = serde_json::from_value(value).unwrap() else {
            panic!()
        };
        assert!(warnings.is_empty());
    }

    #[test]
    fn whole_document_patch_replaces_everything() {
        let p = TextPatch::whole("new", "old text".chars().count());
        assert_eq!(p.apply("old text").unwrap(), "new");
    }

    #[test]
    fn splices_apply_in_order_on_char_offsets() {
        // "héllo" — é is one char, two bytes; offsets are chars.
        let p = TextPatch::Replace(vec![
            Splice {
                start: 1,
                end: 2,
                text: "e".into(),
            },
            Splice {
                start: 5,
                end: 5,
                text: "!".into(),
            },
        ]);
        assert_eq!(p.apply("héllo").unwrap(), "hello!");
    }

    #[test]
    fn out_of_range_splice_is_invalid() {
        let p = TextPatch::Replace(vec![Splice {
            start: 0,
            end: 9,
            text: String::new(),
        }]);
        assert!(matches!(p.apply("abc"), Err(SourceError::Invalid(_))));
    }
}
