//! SQLite source over `rusqlite`.
//!
//! - Ids: `sqlite:<absolute path>`; nodes derived from `""` (database),
//!   `"table:<name>"`, `"column:<table>.<name>"`.
//! - `Children(database)` → tables and views; `Children(table)` → columns
//!   (`PRAGMA table_info`), so the explorer and the graph see the schema.
//! - `Query::Text` runs read statements only (see `text::classify`), capped
//!   at [`ROW_CAP`] rows; results are a `Table` of `Value`s.
//! - `Query::All` returns the schema graph.
//! - `Connection` is `!Sync`, so it lives behind a `Mutex` and queries run on
//!   `spawn_blocking`.

use crate::text::{classify, Statement};
use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, Node, NodeId, NodeKind, Query, QueryResult, Source,
    SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction, Value,
    Version,
};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

pub const ROW_CAP: usize = 500;

pub struct SqliteSource {
    id: SourceId,
    path: PathBuf,
    conn: Arc<Mutex<Connection>>,
    /// `NodeId → native key`, filled as nodes are listed.
    known: RwLock<HashMap<NodeId, String>>,
}

impl SqliteSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| SourceError::Io(e.to_string()))?;
        let id = SourceId::new(format!("sqlite:{}", path.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Ok(Self {
            id,
            path,
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
            let columns: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
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
                for i in 0..n {
                    out.push(match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(i) => Value::Int(i),
                        ValueRef::Real(f) => Value::Float(f),
                        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
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
                match classify(&text) {
                    Statement::Read => {}
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

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "SQLite sources are read-only in this milestone".into(),
        ))
    }
}
