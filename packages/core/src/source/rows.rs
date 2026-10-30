//! Typed, optimistic database row operations. SQL text is never a write API.
use crate::{SourceError, Value, Version};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Native key columns and values, as returned by the authoritative source.
/// A key can retain raw text where the displayed cell has a DATE projection.
pub type RowKey = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowSnapshot {
    pub key: RowKey,
    pub values: BTreeMap<String, Value>,
    /// Original native values where display typing changes the representation.
    /// Undo restores these (for example a SQLite DATE stored as date-only text).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stored: BTreeMap<String, Value>,
    pub version: Version,
}

/// A bounded table page, with source-checked identities rather than query aliases.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowPage {
    pub columns: Vec<String>,
    pub rows: Vec<RowSnapshot>,
    pub refusal: Option<String>,
    pub truncated: bool,
    #[serde(default)]
    pub next: Option<super::TableCursor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowChange {
    Update(BTreeMap<String, Value>),
    /// Inserts name the complete usable key. `expected: None` asserts absence.
    Insert(BTreeMap<String, Value>),
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowOp {
    pub table: String,
    pub key: RowKey,
    /// None means the keyed row must not exist (insert only).
    pub expected: Option<Version>,
    pub change: RowChange,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowBatch {
    pub ops: Vec<RowOp>,
}

impl RowBatch {
    /// Bound work before accessing the database, including untrusted remote input.
    pub fn validate(&self) -> Result<(), SourceError> {
        if self.ops.is_empty() || self.ops.len() > 64 {
            return Err(SourceError::Invalid(
                "row batch requires 1..64 operations".into(),
            ));
        }
        let mut seen: Vec<(&str, &RowKey)> = Vec::new();
        for op in &self.ops {
            if seen
                .iter()
                .any(|(table, key)| *table == op.table && *key == &op.key)
            {
                return Err(SourceError::Invalid(
                    "combine changes to each row into one operation".into(),
                ));
            }
            seen.push((&op.table, &op.key));
            if op.table.is_empty()
                || op.table.len() > 1024
                || op.key.is_empty()
                || op.key.len() > 64
            {
                return Err(SourceError::Invalid("invalid row identity".into()));
            }
            if matches!(op.change, RowChange::Insert(_)) != op.expected.is_none() {
                return Err(SourceError::Invalid(
                    "insert requires absence; update/delete require a version".into(),
                ));
            }
        }
        if serde_json::to_vec(self)
            .map_err(|e| SourceError::Invalid(e.to_string()))?
            .len()
            > 4 * 1024 * 1024
        {
            return Err(SourceError::Invalid("row batch exceeds 4 MiB".into()));
        }
        Ok(())
    }
}
