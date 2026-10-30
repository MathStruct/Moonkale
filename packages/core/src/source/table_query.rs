//! Bounded typed table requests. All sort directions keep NULLs last.
use crate::{RowKey, SourceError, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCapabilities {
    pub sort: bool,
    pub filter: bool,
    pub keyset: bool,
}
impl TableCapabilities {
    pub const SQL: Self = Self {
        sort: true,
        filter: true,
        keyset: true,
    };
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSort {
    pub column: String,
    pub descending: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TablePredicate {
    Eq(Value),
    Lt(Value),
    Le(Value),
    Gt(Value),
    Ge(Value),
    Contains(String),
    Prefix(String),
    IsNull,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableFilter {
    pub column: String,
    pub predicate: TablePredicate,
}
/// Native sort values and the complete authoritative row key. Cursors belong
/// to the same table/sort/filter request; changing controls clears the cursor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCursor {
    pub values: Vec<Value>,
    pub key: RowKey,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableQuery {
    pub sort: Vec<TableSort>,
    /// Conjunction (AND); no SQL fragments or expression language.
    pub filters: Vec<TableFilter>,
    pub limit: usize,
    pub after: Option<TableCursor>,
}
impl Default for TableQuery {
    fn default() -> Self {
        Self {
            sort: vec![],
            filters: vec![],
            limit: 1000,
            after: None,
        }
    }
}
impl TableQuery {
    pub fn validate(&self) -> Result<(), SourceError> {
        if !(1..=1000).contains(&self.limit)
            || self.sort.len() > 8
            || self.filters.len() > 32
            || self
                .sort
                .iter()
                .any(|s| s.column.is_empty() || s.column.len() > 1024)
            || self
                .filters
                .iter()
                .any(|s| s.column.is_empty() || s.column.len() > 1024)
            || self
                .after
                .as_ref()
                .is_some_and(|c| c.values.len() != self.sort.len() || c.key.len() > 64)
            || serde_json::to_vec(self)
                .map_err(|e| SourceError::Invalid(e.to_string()))?
                .len()
                > 64 * 1024
        {
            return Err(SourceError::Invalid(
                "invalid or oversized table query".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        if self.sort.iter().any(|s| !seen.insert(&s.column)) {
            return Err(SourceError::Invalid("duplicate sort column".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_old_wire_defaults() {
        TableQuery::default().validate().unwrap();
        let sort = TableSort {
            column: "x".into(),
            descending: false,
        };
        for q in [
            TableQuery {
                limit: 0,
                ..Default::default()
            },
            TableQuery {
                sort: vec![sort.clone(); 9],
                ..Default::default()
            },
            TableQuery {
                sort: vec![sort.clone(); 2],
                ..Default::default()
            },
            TableQuery {
                filters: vec![TableFilter {
                    column: "x".into(),
                    predicate: TablePredicate::Contains("a".repeat(65536)),
                }],
                ..Default::default()
            },
            TableQuery {
                after: Some(TableCursor {
                    values: vec![Value::Int(1)],
                    key: Default::default(),
                }),
                ..Default::default()
            },
        ] {
            assert!(q.validate().is_err());
        }
        let page: crate::RowPage =
            serde_json::from_str(r#"{"columns":[],"rows":[],"refusal":null,"truncated":false}"#)
                .unwrap();
        assert!(page.next.is_none());
        let caps: crate::Capabilities =
            serde_json::from_str(r#"{"read":true,"write":false,"watch":false,"text_query":null}"#)
                .unwrap();
        assert_eq!(caps.table_query, TableCapabilities::default());
    }
}
