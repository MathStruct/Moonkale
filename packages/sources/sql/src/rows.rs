//! Shared typed row planner. Every identifier comes from fresh catalog metadata;
//! only generated statement shapes reach the drivers and all values are bound.
use moonkale_core::{
    async_trait, RowBatch, RowChange, RowKey, RowPage, RowSnapshot, SourceError, Value, Version,
};

type Result<T> = std::result::Result<T, SourceError>;

pub(crate) fn err(e: impl std::fmt::Display) -> SourceError {
    SourceError::Invalid(e.to_string())
}
pub(crate) fn budget(used: &mut usize, value: &Value) -> Result<()> {
    let bytes = match value {
        Value::Text(s) => s.len(),
        Value::Vector(v) => v.as_slice().len().saturating_mul(8),
        _ => 32,
    };
    *used = used.saturating_add(bytes);
    if bytes > 512 * 1024 || *used > 4 * 1024 * 1024 {
        return Err(unsupported("row/catalog value budget exceeded"));
    }
    Ok(())
}
pub(crate) fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn unsupported(s: &str) -> SourceError {
    SourceError::Unsupported(s.into())
}

pub(crate) struct Schema {
    pub columns: Vec<String>,
    pub keys: Vec<String>,
    pub refusal: Option<String>,
}

#[async_trait]
pub(crate) trait Db: Send {
    async fn query(&mut self, sql: &str, params: Vec<Value>) -> Result<Vec<Vec<Value>>>;
    async fn execute(&mut self, sql: &str, params: Vec<Value>) -> Result<()>;
    async fn schema(&mut self, table: &str) -> Result<Schema>;
    fn begin(&self) -> &'static str {
        "BEGIN IMMEDIATE"
    }
}

fn text(v: &Value) -> String {
    if let Value::Text(s) = v {
        s.clone()
    } else {
        String::new()
    }
}
#[cfg(any(feature = "sqlite", feature = "turso"))]
fn number(v: &Value) -> i64 {
    if let Value::Int(n) = v {
        *n
    } else {
        0
    }
}

/// SQLite-compatible catalog inspection. Virtual tables have extension-defined
/// side effects and are refused along with generated columns and views.
#[cfg(any(feature = "sqlite", feature = "turso"))]
pub(crate) async fn sqlite_schema(db: &mut impl Db, table: &str) -> Result<Schema> {
    let meta = db
        .query(
            "SELECT type, sql FROM main.sqlite_schema WHERE name = ? AND name NOT LIKE 'sqlite_%'",
            vec![Value::Text(table.into())],
        )
        .await?;
    let Some(meta) = meta.first() else {
        return Err(SourceError::NotFound);
    };
    let mut refusal = (text(&meta[0]) != "table"
        || text(&meta[1])
            .to_uppercase()
            .contains("CREATE VIRTUAL TABLE"))
    .then(|| "views and virtual tables are read-only".into());
    let rows = db
        .query(
            &format!("PRAGMA main.table_xinfo({})", quote(table)),
            vec![],
        )
        .await?;
    let columns: Vec<_> = rows.iter().map(|r| text(&r[1])).collect();
    if rows
        .iter()
        .any(|r| r.get(6).is_some_and(|v| number(v) != 0))
    {
        refusal = Some("generated columns are read-only".into());
    }
    let mut pk: Vec<_> = rows.iter().filter(|r| number(&r[5]) > 0).collect();
    pk.sort_by_key(|r| number(&r[5]));
    let mut keys: Vec<_> = pk.iter().map(|r| text(&r[1])).collect();
    let integer_alias = pk.len() == 1 && text(&pk[0][2]).eq_ignore_ascii_case("INTEGER");
    if !integer_alias && pk.iter().any(|r| number(&r[3]) == 0) {
        keys.clear();
    }
    // Nullable SQLite primary keys do not establish row identity. A rowid
    // fallback is safe only when no real column shadows the hidden alias.
    if keys.is_empty() && !text(&meta[1]).to_uppercase().contains("WITHOUT ROWID") {
        if let Some(alias) = ["_rowid_", "rowid", "oid"]
            .into_iter()
            .find(|a| !columns.iter().any(|c| c.eq_ignore_ascii_case(a)))
        {
            keys.push(alias.into());
        }
    }
    if keys.is_empty() {
        refusal = Some("table has no usable primary key or stable rowid".into());
    }
    Ok(Schema {
        columns,
        keys,
        refusal,
    })
}

#[cfg(feature = "duckdb")]
pub(crate) async fn duck_schema(db: &mut impl Db, table: &str) -> Result<Schema> {
    let tables = db.query("SELECT table_type FROM information_schema.tables WHERE table_catalog = current_database() AND table_schema = 'main' AND table_name = ?", vec![Value::Text(table.into())]).await?;
    let Some(meta) = tables.first() else {
        return Err(SourceError::NotFound);
    };
    let rows = db.query("SELECT column_name, column_default FROM information_schema.columns WHERE table_catalog = current_database() AND table_schema = 'main' AND table_name = ? ORDER BY ordinal_position", vec![Value::Text(table.into())]).await?;
    let columns: Vec<_> = rows.iter().map(|r| text(&r[0])).collect();
    let mut refusal = (text(&meta[0]) != "BASE TABLE").then(|| "views are read-only".into());
    // Catalog functions accept the name as data; pragma_table_info reparses
    // qualified-name strings and cannot faithfully address embedded quotes.
    let primary = db.query("SELECT unnest(constraint_column_names) FROM duckdb_constraints() WHERE database_name = current_database() AND schema_name = 'main' AND table_name = ? AND constraint_type = 'PRIMARY KEY'",vec![Value::Text(table.into())]).await?;
    let mut keys: Vec<_> = primary.iter().map(|r| text(&r[0])).collect();
    let ddl = db.query("SELECT sql FROM duckdb_tables() WHERE database_name = current_database() AND schema_name = 'main' AND table_name = ?", vec![Value::Text(table.into())]).await?;
    if ddl
        .first()
        .is_some_and(|r| text(&r[0]).to_uppercase().contains("GENERATED ALWAYS"))
    {
        refusal = Some("generated columns are read-only".into());
    }
    // DuckDB rowid is a physical locator, not a durable identity across checkpoints.
    if keys.is_empty() && refusal.is_none() {
        refusal = Some("DuckDB writes require a primary key".into());
    }
    keys.sort();
    Ok(Schema {
        columns,
        keys,
        refusal,
    })
}

fn predicate(schema: &Schema, key: &RowKey) -> Result<(String, Vec<Value>)> {
    if key.len() != schema.keys.len() || schema.keys.iter().any(|k| !key.contains_key(k)) {
        return Err(unsupported("row key does not match the source schema"));
    }
    let mut params = Vec::new();
    let mut parts = Vec::new();
    for k in &schema.keys {
        let value = key[k].clone();
        if matches!(
            value,
            Value::Null | Value::Bytes { .. } | Value::Vector(_) | Value::Ref { .. }
        ) {
            return Err(unsupported("row key is null or not a supported scalar"));
        }
        parts.push(format!("{} = ?", quote(k)));
        params.push(value);
    }
    Ok((parts.join(" AND "), params))
}
fn native_columns(schema: &Schema) -> Vec<String> {
    let mut columns = schema.columns.clone();
    for key in &schema.keys {
        if !columns.contains(key) {
            columns.push(key.clone());
        }
    }
    columns
}
fn snapshot(table: &str, schema: &Schema, values: Vec<Value>) -> Result<RowSnapshot> {
    let native = native_columns(schema);
    if values.len() != schema.columns.len() + native.len() {
        return Err(err("row schema changed"));
    }
    // Hash native values too: formatting a SQLite DATE differently is still an
    // external write even when the localized display represents the same instant.
    let encoded = serde_json::to_vec(&(table, &schema.columns, &native, &values)).map_err(err)?;
    let digest = blake3::hash(&encoded);
    let version = Version(u64::from_le_bytes(
        digest.as_bytes()[..8].try_into().unwrap(),
    ));
    let raw = &values[schema.columns.len()..];
    let key = schema
        .keys
        .iter()
        .map(|key| {
            let index = native.iter().position(|name| name == key).unwrap();
            (key.clone(), raw[index].clone())
        })
        .collect();
    let stored = schema
        .columns
        .iter()
        .enumerate()
        .filter(|(i, _)| values[*i] != raw[*i])
        .map(|(i, name)| (name.clone(), raw[i].clone()))
        .collect();
    let values = schema.columns.iter().cloned().zip(values).collect();
    Ok(RowSnapshot {
        key,
        values,
        stored,
        version,
    })
}
fn select(table: &str, schema: &Schema) -> String {
    let mut columns: Vec<_> = schema.columns.iter().map(|c| quote(c)).collect();
    // CASE retains native values while dropping declared DATE metadata on
    // SQLite/Turso. DuckDB retains its native typed date/timestamp values.
    columns.extend(
        native_columns(schema)
            .iter()
            .map(|c| format!("CASE WHEN 1 THEN {} END", quote(c))),
    );
    format!("SELECT {} FROM main.{}", columns.join(","), quote(table))
}
async fn fetch(
    db: &mut impl Db,
    table: &str,
    schema: &Schema,
    key: &RowKey,
) -> Result<Option<RowSnapshot>> {
    let (clause, params) = predicate(schema, key)?;
    let rows = db
        .query(
            &format!("{} WHERE {clause} LIMIT 2", select(table, schema)),
            params,
        )
        .await?;
    if rows.len() > 1 {
        return Err(unsupported("key does not identify exactly one row"));
    }
    rows.into_iter()
        .next()
        .map(|v| snapshot(table, schema, v))
        .transpose()
}

pub(crate) async fn query(
    db: &mut impl Db,
    table: String,
    query: moonkale_core::TableQuery,
) -> Result<RowPage> {
    query.validate()?;
    if table.is_empty() || table.len() > 1024 {
        return Err(err("invalid table name"));
    }
    let schema = db.schema(&table).await?;
    if schema.refusal.is_some() {
        return Ok(RowPage {
            columns: schema.columns,
            refusal: schema.refusal,
            ..Default::default()
        });
    }
    let limit = query.limit;
    let (suffix, params) = plan(&schema, &query)?;
    let rows = db
        .query(&format!("{} {suffix}", select(&table, &schema)), params)
        .await?;
    let truncated = rows.len() > limit;
    let rows = rows
        .into_iter()
        .take(limit)
        .map(|r| snapshot(&table, &schema, r))
        .collect::<Result<Vec<_>>>()?;
    let bytes = serde_json::to_vec(&rows).map_err(err)?.len();
    if bytes > 4 * 1024 * 1024 {
        return Err(unsupported("row page exceeds 4 MiB"));
    }
    // Refuse nullable/non-unique keys even on reads so the UI has a reason.
    for row in &rows {
        predicate(&schema, &row.key)?;
    }
    let next = if truncated {
        rows.last().map(|row| moonkale_core::TableCursor {
            values: query
                .sort
                .iter()
                .map(|s| {
                    row.stored
                        .get(&s.column)
                        .or_else(|| row.values.get(&s.column))
                        .or_else(|| row.key.get(&s.column))
                        .expect("schema-checked sort column")
                        .clone()
                })
                .collect(),
            key: row.key.clone(),
        })
    } else {
        None
    };
    Ok(RowPage {
        next,
        columns: schema.columns,
        rows,
        refusal: None,
        truncated,
    })
}

pub(crate) async fn batch(
    db: &mut impl Db,
    batch: RowBatch,
    preview: bool,
) -> Result<Vec<Option<RowSnapshot>>> {
    batch.validate()?;
    db.execute(db.begin(), vec![]).await?;
    let result = apply(db, batch, preview).await;
    match result {
        Ok(rows) if !preview => {
            if let Err(e) = db.execute("COMMIT", vec![]).await {
                let _ = db.execute("ROLLBACK", vec![]).await;
                return Err(e);
            }
            Ok(rows)
        }
        result => {
            db.execute("ROLLBACK", vec![]).await?;
            result
        }
    }
}
async fn apply(
    db: &mut impl Db,
    batch: RowBatch,
    preview: bool,
) -> Result<Vec<Option<RowSnapshot>>> {
    let mut plans = Vec::new();
    let mut out = Vec::new();
    let mut bytes = 0usize;
    for op in batch.ops {
        let schema = db.schema(&op.table).await?;
        if let Some(reason) = &schema.refusal {
            return Err(unsupported(reason));
        }
        let old = fetch(db, &op.table, &schema, &op.key).await?;
        if op.expected != old.as_ref().map(|r| r.version) {
            return Err(SourceError::Conflict {
                expected: op.expected.unwrap_or_default(),
                actual: old.as_ref().map(|r| r.version).unwrap_or_default(),
            });
        }
        let table = format!("main.{}", quote(&op.table));
        let (where_sql, key_params) = predicate(&schema, &op.key)?;
        let (sql, params) = match &op.change {
            RowChange::Update(values) | RowChange::Insert(values) => {
                if values.is_empty()
                    || values.len() > 1024
                    || values
                        .keys()
                        .any(|c| !schema.columns.contains(c) && !schema.keys.contains(c))
                {
                    return Err(unsupported("unknown or empty column set"));
                }
                for (c, v) in values {
                    if schema.keys.contains(c) && op.key.get(c) != Some(v) {
                        return Err(unsupported("key changes require delete and insert"));
                    }
                }
                let cols: Vec<_> = values.keys().map(|c| quote(c)).collect();
                let mut params: Vec<_> = values.values().cloned().collect();
                let sql = if matches!(op.change, RowChange::Insert(_)) {
                    if schema.keys.iter().any(|k| values.get(k) != op.key.get(k)) {
                        return Err(unsupported("insert must include the complete key"));
                    }
                    format!(
                        "INSERT INTO {table} ({}) VALUES ({})",
                        cols.join(","),
                        vec!["?"; cols.len()].join(",")
                    )
                } else {
                    params.extend(key_params);
                    format!(
                        "UPDATE {table} SET {} WHERE {where_sql}",
                        cols.iter()
                            .map(|c| format!("{c} = ?"))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                };
                (sql, params)
            }
            RowChange::Delete => (format!("DELETE FROM {table} WHERE {where_sql}"), key_params),
        };
        // EXPLAIN prepares and binds without executing mutations/defaults.
        // Validate every base and statement before executing the first write.
        db.query(&format!("EXPLAIN {sql}"), params.clone()).await?;
        plans.push((op, schema, sql, params));
    }
    if preview {
        return Ok(Vec::new());
    }
    for (op, schema, sql, params) in plans {
        db.execute(&sql, params).await?;
        let after = fetch(db, &op.table, &schema, &op.key).await?;
        bytes = bytes.saturating_add(serde_json::to_vec(&after).map_err(err)?.len());
        if bytes > 4 * 1024 * 1024 {
            return Err(unsupported("row result budget exceeded"));
        }
        out.push(after);
    }
    Ok(out)
}

/// Lexicographic seek, with explicit NULLS LAST in both directions. The key
/// suffix makes duplicate user sort values stable, including composite keys.
fn plan(schema: &Schema, query: &moonkale_core::TableQuery) -> Result<(String, Vec<Value>)> {
    use moonkale_core::TablePredicate as P;
    let scalar = |v: &Value| -> Result<()> {
        match v {
            Value::Int(_) | Value::Bool(_) | Value::Text(_) | Value::DateTime(_) => Ok(()),
            Value::Float(n) if n.is_finite() => Ok(()),
            _ => Err(unsupported("table predicates require a finite scalar")),
        }
    };
    let column = |c: &str| -> Result<String> {
        if !schema.columns.iter().chain(&schema.keys).any(|s| s == c) {
            return Err(err("unknown table column"));
        }
        Ok(quote(c))
    };
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    for filter in &query.filters {
        let c = column(&filter.column)?;
        clauses.push(match &filter.predicate {
            P::IsNull => format!("{c} IS NULL"),
            P::Contains(s) => {
                params.push(Value::Text(s.clone()));
                format!("instr({c}, ?) > 0")
            }
            P::Prefix(s) => {
                params.extend([Value::Int(s.chars().count() as i64), Value::Text(s.clone())]);
                format!("substr({c}, 1, ?) = ?")
            }
            p => {
                let (op, v) = match p {
                    P::Eq(v) => ("=", v),
                    P::Lt(v) => ("<", v),
                    P::Le(v) => ("<=", v),
                    P::Gt(v) => (">", v),
                    P::Ge(v) => (">=", v),
                    _ => unreachable!(),
                };
                scalar(v)?;
                params.push(v.clone());
                format!("{c} {op} ?")
            }
        });
    }
    let mut order = Vec::new();
    let mut seek_columns = Vec::new();
    for sort in &query.sort {
        let c = column(&sort.column)?;
        order.push(format!(
            "{c} {} NULLS LAST",
            if sort.descending { "DESC" } else { "ASC" }
        ));
        seek_columns.push((c, sort.descending));
    }
    // Repeating a user-sorted key is harmless and keeps cursor shape explicit.
    for key in &schema.keys {
        let c = quote(key);
        order.push(format!("{c} ASC NULLS LAST"));
        seek_columns.push((c, false));
    }
    if let Some(cursor) = &query.after {
        predicate(schema, &cursor.key)?;
        let mut values = cursor.values.clone();
        values.extend(schema.keys.iter().map(|k| cursor.key[k].clone()));
        let mut alternatives = Vec::new();
        for i in 0..values.len() {
            if values[i] == Value::Null {
                continue;
            } // NULL is the final group.
            scalar(&values[i])?;
            let mut parts = Vec::new();
            for j in 0..i {
                let c = &seek_columns[j].0;
                if values[j] == Value::Null {
                    parts.push(format!("{c} IS NULL"));
                } else {
                    scalar(&values[j])?;
                    parts.push(format!("{c} = ?"));
                    params.push(values[j].clone());
                }
            }
            let (c, desc) = &seek_columns[i];
            parts.push(format!(
                "({c} {} ? OR {c} IS NULL)",
                if *desc { "<" } else { ">" }
            ));
            params.push(values[i].clone());
            alternatives.push(format!("({})", parts.join(" AND ")));
        }
        clauses.push(if alternatives.is_empty() {
            "FALSE".into()
        } else {
            format!("({})", alternatives.join(" OR "))
        });
    }
    params.push(Value::Int((query.limit + 1) as i64));
    Ok((
        format!(
            "{} ORDER BY {} LIMIT ?",
            if clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", clauses.join(" AND "))
            },
            order.join(",")
        ),
        params,
    ))
}
