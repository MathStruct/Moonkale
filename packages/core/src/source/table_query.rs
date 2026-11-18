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

pub const MAX_AGGREGATE_BUCKETS: usize = 100;

/// Which aggregate a [`TableAggregate`] computes in the source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TableAggregateKind {
    /// Fixed-width (or `log10`) 40-bin histogram over `x`.
    Histogram {
        /// Bin in `log10(x)` space (values `<= 0` are omitted).
        #[serde(default)]
        log: bool,
    },
    /// Sum of numeric `y` grouped by categorical `x` (up to 100 categories).
    Bar {
        /// Numeric value column to sum per category.
        y: String,
    },
}

/// Bounded, injection-safe aggregate request pushed down to a table source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableAggregate {
    /// Column for the x-axis (binned for histogram, grouped for bar).
    pub x: String,
    /// Aggregate kind (histogram or bar).
    pub kind: TableAggregateKind,
    /// Active row filters from the table grid (AND-ed before aggregation).
    #[serde(default)]
    pub filters: Vec<TableFilter>,
}

impl TableAggregate {
    /// Check all bounds before any driver touches the request.
    pub fn validate(&self) -> Result<(), SourceError> {
        let bad_col = |c: &str| c.is_empty() || c.len() > 1024;
        let bad_y = match &self.kind {
            TableAggregateKind::Histogram { .. } => false,
            TableAggregateKind::Bar { y } => bad_col(y),
        };
        if bad_col(&self.x)
            || bad_y
            || self.filters.len() > 32
            || self.filters.iter().any(|f| bad_col(&f.column))
            || serde_json::to_vec(self)
                .map_err(|e| SourceError::Invalid(e.to_string()))?
                .len()
                > 64 * 1024
        {
            return Err(SourceError::Invalid(
                "invalid or oversized table aggregate".into(),
            ));
        }
        Ok(())
    }
}

/// One aggregated bin or category returned by [`crate::Source::aggregate_rows`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableAggregateBucket {
    /// Category key (`x` value) for bar charts, or `Value::Int(bin_index)` for histograms.
    pub key: Value,
    /// Bin midpoint (in plot x-space: `log10` when `log` is true) for histograms, or `0.0` for bar charts.
    pub x_mid: f64,
    /// Aggregated height: row count in the bin for histograms, or `SUM(y)` for bar charts.
    pub value: f64,
    /// Number of rows that contributed to this bucket.
    pub count: u64,
}

/// Result of [`crate::Source::aggregate_rows`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableAggregateResult {
    /// Ordered buckets (up to 40 for histograms, up to 100 for bars).
    pub buckets: Vec<TableAggregateBucket>,
    /// Total rows matching `filters` (before omitting non-numeric/non-positive/uncapped rows).
    pub total_rows: u64,
    /// Rows matching `filters` that were omitted (null/non-numeric, `<= 0` on log scale, or beyond 100 categories).
    pub omitted: u64,
    /// Histogram domain minimum (in `log10` space when `log` is true), if applicable.
    #[serde(default)]
    pub lo: Option<f64>,
    /// Histogram domain maximum (in `log10` space when `log` is true), if applicable.
    #[serde(default)]
    pub hi: Option<f64>,
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
