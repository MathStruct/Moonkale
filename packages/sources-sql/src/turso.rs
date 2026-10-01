//! Turso source (Milestone 17): the in-process Rust rewrite of SQLite
//! (`turso` crate, beta; async I/O, MVCC). Same file format and dialect as
//! SQLite, so the lifting mirrors [`crate::sqlite`]:
//!
//! - Ids: `turso:<absolute path>`; nodes derived from `""` (database),
//!   `"table:<name>"`, `"column:<table>.<name>"`.
//! - `Children(database)` → tables and views; `Children(table)` → columns.
//! - `Query::Text` runs read statements only (`Source::classify`, the shared rules in `moonkale_core::source::risk`), capped at
//!   [`ROW_CAP`] rows. `Query::All` → the schema graph.
//! - Opened by extension (`*.turso`): a Turso file *is* a SQLite file, and
//!   `.db`/`.sqlite` keep opening with SQLite.
//! - Read-only for now, like SQLite: writes are the next step.

use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, Node, NodeId, NodeKind, Query, QueryResult, Source,
    SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction, Value,
    Version,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub const ROW_CAP: usize = 500;

pub struct TursoSource {
    id: SourceId,
    path: PathBuf,
    db: turso::Database,
    known: RwLock<HashMap<NodeId, String>>,
}

fn err(e: impl std::fmt::Display) -> SourceError {
    SourceError::Invalid(e.to_string())
}

impl TursoSource {
    /// Open an existing Turso/SQLite file (never creates one).
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        let db = turso::Builder::new_local(&path.to_string_lossy())
            .build()
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?;
        let id = SourceId::new(format!("turso:{}", path.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Ok(Self {
            id,
            path,
            db,
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

    /// Run a statement, at most `cap` rows.
    async fn rows(&self, sql: &str, cap: usize) -> Result<Table, SourceError> {
        let conn = self.db.connect().map_err(err)?;
        let mut rows = conn.query(sql, ()).await.map_err(err)?;
        let columns = rows.column_names();
        let n = columns.len();
        let mut out = Vec::new();
        let mut truncated = false;
        while let Some(row) = rows.next().await.map_err(err)? {
            if out.len() >= cap {
                truncated = true;
                break;
            }
            let mut r = Vec::with_capacity(n);
            for i in 0..n {
                r.push(match row.get_value(i).map_err(err)? {
                    turso::Value::Null => Value::Null,
                    turso::Value::Integer(i) => Value::Int(i),
                    turso::Value::Real(f) => Value::Float(f),
                    turso::Value::Text(t) => Value::Text(t),
                    turso::Value::Blob(b) => Value::Bytes {
                        len: b.len() as u64,
                    },
                });
            }
            out.push(r);
        }
        Ok(Table {
            columns,
            rows: out,
            truncated,
        })
    }

    fn text_at(t: &Table, row: usize, col: usize) -> String {
        match t.rows.get(row).and_then(|r| r.get(col)) {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        }
    }

    async fn table_names(&self) -> Result<Vec<(String, String)>, SourceError> {
        let t = self
            .rows(
                "SELECT name, type FROM sqlite_schema WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
                100_000,
            )
            .await?;
        Ok((0..t.rows.len())
            .map(|i| (Self::text_at(&t, i, 0), Self::text_at(&t, i, 1)))
            .collect())
    }

    async fn columns(&self, table: &str) -> Result<Vec<(String, String)>, SourceError> {
        let t = self
            .rows(
                &format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\"")),
                100_000,
            )
            .await?;
        Ok((0..t.rows.len())
            .map(|i| (Self::text_at(&t, i, 1), Self::text_at(&t, i, 2)))
            .collect())
    }

    fn table_node(&self, name: &str, ty: &str) -> Node {
        self.make(
            &format!("table:{name}"),
            NodeKind::Table,
            if ty == "view" {
                format!("{name} (view)")
            } else {
                name.to_string()
            },
        )
    }

    fn column_node(&self, table: &str, col: &str, ty: &str) -> Node {
        self.make(
            &format!("column:{table}.{col}"),
            NodeKind::Column,
            if ty.is_empty() {
                col.to_string()
            } else {
                format!("{col}: {ty}")
            },
        )
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for TursoSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            display_name: self.database_node().label,
            family: SourceFamily::Sql,
            capabilities: Capabilities {
                read: true,
                write: false,
                watch: false,
                text_query: Some(TextDialect::Sql),
            },
            root: self.root_id(),
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
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
                        let t = self.table_node(&name, &ty);
                        res.edges.push(Edge::contains(&self.id, id, t.id));
                        res.nodes.push(t);
                    }
                } else if let Some(table) = key.strip_prefix("table:") {
                    for (col, ty) in self.columns(table).await? {
                        let c = self.column_node(table, &col, &ty);
                        res.edges.push(Edge::contains(&self.id, id, c.id));
                        res.nodes.push(c);
                    }
                }
                Ok(res)
            }
            Query::All { limit, .. } => {
                let db = self.database_node();
                let mut res = QueryResult::single(db.clone());
                for (name, ty) in self.table_names().await? {
                    let t = self.table_node(&name, &ty);
                    res.edges.push(Edge::contains(&self.id, db.id, t.id));
                    for (col, cty) in self.columns(&name).await? {
                        let c = self.column_node(&name, &col, &cty);
                        res.edges.push(Edge::contains(&self.id, t.id, c.id));
                        res.nodes.push(c);
                    }
                    res.nodes.push(t);
                }
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
                let table = self.rows(&text, ROW_CAP).await?;
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

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "Turso sources are read-only in this milestone".into(),
        ))
    }
}
