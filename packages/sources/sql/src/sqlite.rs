//! SQLite source over `rusqlite`.
//!
//! - Ids: `sqlite:<absolute path>`; nodes derived from `""` (database),
//!   `"table:<name>"`, `"column:<table>.<name>"`.
//! - `Children(database)` → tables and views; `Children(table)` → columns
//!   (`PRAGMA table_info`), so the explorer and the graph see the schema.
//! - `Query::Text` runs read statements only (`Source::classify`, the shared rules in `moonkale_core::source::risk`), capped
//!   at [`ROW_CAP`] rows; results are a `Table` of `Value`s.
//! - `Query::All` returns the schema graph.
//! - `Connection` is `!Sync`, so it lives behind a `Mutex` and queries run on
//!   `spawn_blocking`.

use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, Node, NodeId, NodeKind, Query, QueryResult, Source,
    SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction, Value,
    Version,
};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

/// The most rows one text query returns. The table editor's grid is
/// virtualised, so the cap guards memory and transfer, not drawing.
pub const ROW_CAP: usize = 50_000;

pub struct SqliteSource {
    id: SourceId,
    path: PathBuf,
    writable: bool,
    conn: Arc<Mutex<Connection>>,
    /// `NodeId → native key`, filled as nodes are listed.
    known: RwLock<HashMap<NodeId, String>>,
}

impl SqliteSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        if path.to_str().is_none() {
            return Err(SourceError::Invalid(
                "database paths require an unambiguous UTF-8 source identity".into(),
            ));
        }
        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .or_else(|_| {
            Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
        })
        .map_err(|e| SourceError::Io(e.to_string()))?;
        let writable = !conn.is_readonly("main").unwrap_or(true);
        let id = SourceId::new(format!("sqlite:{}", path.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Ok(Self {
            id,
            path,
            writable,
            conn: Arc::new(Mutex::new(conn)),
            known: RwLock::new(known),
        })
    }

    pub fn root_id(&self) -> NodeId {
        NodeId::derive(&self.id, "")
    }

    fn node_id(&self, key: &str) -> NodeId {
        let id = NodeId::derive(&self.id, key);
        self.known
            .write()
            .unwrap()
            .entry(id)
            .or_insert_with(|| key.to_string());
        id
    }

    fn key_of(&self, id: NodeId) -> Result<String, SourceError> {
        self.known
            .read()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(SourceError::NotFound)
    }

    fn make(&self, key: &str, kind: NodeKind, label: String) -> Node {
        Node {
            props: Default::default(),
            id: self.node_id(key),
            source: self.id.clone(),
            kind,
            label,
            native_key: key.to_string(),
            content: None,
            version: Version::default(),
        }
    }

    fn database_node(&self) -> Node {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "database".into());
        self.make("", NodeKind::Database, name)
    }

    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, SourceError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let c = conn.lock().unwrap();
            f(&c)
        })
        .await
        .map_err(|e| SourceError::Io(e.to_string()))?
        .map_err(|e| SourceError::Invalid(e.to_string()))
    }

    async fn table_names(&self) -> Result<Vec<(String, String)>, SourceError> {
        self.blocking(|c| {
            let mut st = c.prepare("SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect()
        })
        .await
    }

    async fn columns(&self, table: String) -> Result<Vec<(String, String)>, SourceError> {
        self.blocking(move |c| {
            let mut st = c.prepare(&format!(
                "PRAGMA table_info(\"{}\")",
                table.replace('"', "\"\"")
            ))?;
            let rows =
                st.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            rows.collect()
        })
        .await
    }

    async fn run_read(&self, sql: String) -> Result<Table, SourceError> {
        self.blocking(move |c| {
            let mut st = c.prepare(&sql)?;
            if !st.readonly() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let columns: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
            let declared: Vec<_> = st
                .columns()
                .iter()
                .map(|c| c.decl_type().map(str::to_owned))
                .collect();
            let n = columns.len();
            let mut rows = Vec::new();
            let mut truncated = false;
            let mut it = st.query([])?;
            while let Some(r) = it.next()? {
                if rows.len() >= ROW_CAP {
                    truncated = true;
                    break;
                }
                let mut out = Vec::with_capacity(n);
                for (i, declared_type) in declared.iter().enumerate().take(n) {
                    out.push(match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(i) => Value::Int(i),
                        ValueRef::Real(f) => Value::Float(f),
                        ValueRef::Text(t) => crate::typed_text(
                            String::from_utf8_lossy(t).into_owned(),
                            declared_type.as_deref(),
                        ),
                        ValueRef::Blob(b) => Value::Bytes {
                            len: b.len() as u64,
                        },
                    });
                }
                rows.push(out);
            }
            Ok(Table {
                columns,
                rows,
                truncated,
            })
        })
        .await
    }

    /// The schema as a graph: database → tables → columns.
    async fn schema_graph(&self) -> Result<QueryResult, SourceError> {
        let db = self.database_node();
        let mut res = QueryResult::single(db.clone());
        for (name, ty) in self.table_names().await? {
            let t = self.make(
                &format!("table:{name}"),
                NodeKind::Table,
                if ty == "view" {
                    format!("{name} (view)")
                } else {
                    name.clone()
                },
            );
            res.edges.push(Edge::contains(&self.id, db.id, t.id));
            for (col, cty) in self.columns(name.clone()).await? {
                let c = self.make(
                    &format!("column:{name}.{col}"),
                    NodeKind::Column,
                    if cty.is_empty() {
                        col
                    } else {
                        format!("{col}: {cty}")
                    },
                );
                res.edges.push(Edge::contains(&self.id, t.id, c.id));
                res.nodes.push(c);
            }
            res.nodes.push(t);
        }
        Ok(res)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for SqliteSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            truth: Default::default(),
            id: self.id.clone(),
            display_name: self.database_node().label,
            family: SourceFamily::Sql,
            capabilities: Capabilities {
                read: true,
                write: self.writable,
                watch: false,
                text_query: Some(TextDialect::Sql),
                extra_text_query: None,
                atomic_text: false,
                atomic_rows: self.writable,
            },
            root: self.root_id(),
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Coarse { .. } => Err(moonkale_core::SourceError::Unsupported(
                "hierarchy coarsening".into(),
            )),
            Query::Node(id) => {
                let key = self.key_of(id)?;
                let node = if key.is_empty() {
                    self.database_node()
                } else if let Some(t) = key.strip_prefix("table:") {
                    self.make(&key, NodeKind::Table, t.to_string())
                } else if let Some(c) = key.strip_prefix("column:") {
                    self.make(
                        &key,
                        NodeKind::Column,
                        c.rsplit('.').next().unwrap_or(c).to_string(),
                    )
                } else {
                    return Err(SourceError::NotFound);
                };
                Ok(QueryResult::single(node))
            }
            Query::Children(id) | Query::Neighbours { node: id, .. } => {
                let key = self.key_of(id)?;
                let mut res = QueryResult::default();
                if key.is_empty() {
                    for (name, ty) in self.table_names().await? {
                        let t = self.make(
                            &format!("table:{name}"),
                            NodeKind::Table,
                            if ty == "view" {
                                format!("{name} (view)")
                            } else {
                                name
                            },
                        );
                        res.edges.push(Edge::contains(&self.id, id, t.id));
                        res.nodes.push(t);
                    }
                } else if let Some(table) = key.strip_prefix("table:") {
                    for (col, cty) in self.columns(table.to_string()).await? {
                        let c = self.make(
                            &format!("column:{table}.{col}"),
                            NodeKind::Column,
                            if cty.is_empty() {
                                col
                            } else {
                                format!("{col}: {cty}")
                            },
                        );
                        res.edges.push(Edge::contains(&self.id, id, c.id));
                        res.nodes.push(c);
                    }
                }
                Ok(res)
            }
            Query::All { limit, .. } => {
                let mut res = self.schema_graph().await?;
                if res.nodes.len() > limit {
                    res.nodes.truncate(limit);
                    res.truncated = true;
                }
                Ok(res)
            }
            Query::Text { dialect, text } => {
                if dialect != "sql" {
                    return Err(SourceError::Unsupported(format!(
                        "dialect {dialect}; this source speaks sql"
                    )));
                }
                match self.classify("sql", &text) {
                    moonkale_core::Risk::Read => {}
                    other => {
                        return Err(SourceError::Unsupported(format!(
                            "{other:?} statements are not allowed on a read-only source"
                        )))
                    }
                }
                let table = self.run_read(text).await?;
                let truncated = table.truncated;
                Ok(QueryResult {
                    table: Some(table),
                    truncated,
                    ..Default::default()
                })
            }
        }
    }

    async fn fetch_text(&self, _node: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::Unsupported(
            "database nodes have no text; open the table".into(),
        ))
    }

    async fn read_rows(
        &self,
        table: String,
        limit: usize,
    ) -> Result<moonkale_core::RowPage, SourceError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap();
            let mut db = RowDb(&mut guard);
            tokio::runtime::Handle::current().block_on(crate::rows::read(&mut db, table, limit))
        })
        .await
        .map_err(crate::rows::err)?
    }

    async fn preview_rows(&self, batch: moonkale_core::RowBatch) -> Result<(), SourceError> {
        self.row_batch(batch, true).await.map(|_| ())
    }

    async fn apply_rows(
        &self,
        batch: moonkale_core::RowBatch,
    ) -> Result<Vec<Option<moonkale_core::RowSnapshot>>, SourceError> {
        self.row_batch(batch, false).await
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "SQL writes require typed row operations".into(),
        ))
    }
}

impl SqliteSource {
    async fn row_batch(
        &self,
        batch: moonkale_core::RowBatch,
        preview: bool,
    ) -> Result<Vec<Option<moonkale_core::RowSnapshot>>, SourceError> {
        if !self.descriptor().capabilities.atomic_rows {
            return Err(SourceError::Unsupported("read-only database".into()));
        }
        batch.validate()?;
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap();
            let mut db = RowDb(&mut guard);
            tokio::runtime::Handle::current().block_on(crate::rows::batch(&mut db, batch, preview))
        })
        .await
        .map_err(crate::rows::err)?
    }
}

struct RowDb<'a>(&'a mut Connection);

fn bind_row_value(v: Value) -> Result<rusqlite::types::Value, SourceError> {
    use rusqlite::types::Value as D;
    Ok(match v {
        Value::Null => D::Null,
        Value::Int(n) => D::Integer(n),
        Value::Float(n) if n.is_finite() => D::Real(n),
        Value::Bool(b) => D::Integer(i64::from(b)),
        Value::Text(s) => D::Text(s),
        Value::DateTime(d) => D::Text(d.source_text().ok_or_else(|| {
            SourceError::Unsupported("date offset cannot be represented losslessly".into())
        })?),
        _ => {
            return Err(SourceError::Unsupported(
                "this cell type cannot be written".into(),
            ))
        }
    })
}

#[async_trait]
impl crate::rows::Db for RowDb<'_> {
    async fn query(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> Result<Vec<Vec<Value>>, SourceError> {
        let params = params
            .into_iter()
            .map(bind_row_value)
            .collect::<Result<Vec<_>, _>>()?;
        let mut st = self.0.prepare(sql).map_err(crate::rows::err)?;
        let n = st.column_count();
        let declared: Vec<_> = st
            .columns()
            .iter()
            .map(|c| c.decl_type().map(str::to_owned))
            .collect();
        let mut it = st
            .query(rusqlite::params_from_iter(params))
            .map_err(crate::rows::err)?;
        let mut out = Vec::new();
        let mut bytes = 0usize;
        while let Some(r) = it.next().map_err(crate::rows::err)? {
            if out.len() > 4096 {
                return Err(SourceError::Invalid("row/catalog budget exceeded".into()));
            }
            let mut row = Vec::new();
            for (i, declared_type) in declared.iter().enumerate().take(n) {
                let value = match r.get_ref(i).map_err(crate::rows::err)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Text(s) => crate::typed_text(
                        String::from_utf8(s.to_vec()).map_err(crate::rows::err)?,
                        declared_type.as_deref(),
                    ),
                    ValueRef::Integer(n) => Value::Int(n),
                    ValueRef::Real(n) if n.is_finite() => Value::Float(n),
                    _ => {
                        return Err(SourceError::Unsupported(
                            "lossless row versions for this cell type are unavailable".into(),
                        ))
                    }
                };
                crate::rows::budget(&mut bytes, &value)?;
                row.push(value);
            }
            out.push(row);
        }
        Ok(out)
    }
    async fn execute(&mut self, sql: &str, params: Vec<Value>) -> Result<(), SourceError> {
        let params = params
            .into_iter()
            .map(bind_row_value)
            .collect::<Result<Vec<_>, _>>()?;
        self.0
            .execute(sql, rusqlite::params_from_iter(params))
            .map_err(crate::rows::err)?;
        Ok(())
    }
    async fn schema(&mut self, table: &str) -> Result<crate::rows::Schema, SourceError> {
        crate::rows::sqlite_schema(self, table).await
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;
    #[tokio::test]
    async fn schema_dates_are_typed_but_text_and_invalid_dates_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dates.sqlite");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE dates(d DATETIME, plain TEXT, invalid DATE); INSERT INTO dates VALUES('2026-10-23T10:30:00+02:00','2026-10-23','bad');").unwrap();
        drop(c);
        let source = SqliteSource::open(&path).unwrap();
        let table = source.run_read("SELECT * FROM dates".into()).await.unwrap();
        assert!(matches!(&table.rows[0][0], Value::DateTime(d) if d.offset() == Some(7200)));
        assert!(matches!(&table.rows[0][1], Value::Text(_)));
        assert_eq!(table.rows[0][2], Value::Text("bad".into()));
    }
}
