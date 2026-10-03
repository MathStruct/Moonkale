//! [`KvSource`]: any [`KvStore`] as a `Source` (Milestone 17).
//!
//! - Ids: `<kind>:<absolute path>`; nodes derived from `""` (the database)
//!   and `"table:<name>"`.
//! - `Children(database)` → the tables (`NodeKind::Table`, so a click opens
//!   the table editor); a table has no children.
//! - `Query::Text { dialect: "kv" }` → a `Table` of `key`/`value` rows (see
//!   [`crate::kv`]); `Query::All` → database → tables.
//! - Read-only for now: `apply` refuses (writes are the next step).

use crate::kv::{parse, KvQuery, KvStore, KvTable};
use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, Node, NodeId, NodeKind, Query, QueryResult, Source,
    SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction, Value,
    Version,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub struct KvSource<S: KvStore> {
    id: SourceId,
    path: PathBuf,
    store: Arc<S>,
    known: RwLock<HashMap<NodeId, String>>,
}

impl<S: KvStore> KvSource<S> {
    /// `kind` becomes the id's scheme (`redb`, `rocksdb`).
    pub fn new(kind: &str, path: &Path, store: S) -> Self {
        let id = SourceId::new(format!("{kind}:{}", path.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Self {
            id,
            path: path.to_path_buf(),
            store: Arc::new(store),
            known: RwLock::new(known),
        }
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

    fn database_node(&self) -> Node {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "database".into());
        self.node("", NodeKind::Database, name)
    }

    fn node(&self, key: &str, kind: NodeKind, label: String) -> Node {
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

    fn table_node(&self, t: &KvTable) -> Node {
        let mut label = t.name.clone();
        if let Some(n) = t.len {
            label.push_str(&format!(" · {n}"));
        }
        if !t.readable {
            label.push_str(&format!(
                " ({} → {}, not readable)",
                t.key_type, t.value_type
            ));
        }
        self.node(&format!("table:{}", t.name), NodeKind::Table, label)
    }

    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&S) -> Result<T, String> + Send + 'static,
    ) -> Result<T, SourceError> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?
            .map_err(SourceError::Invalid)
    }

    async fn tables(&self) -> Result<Vec<KvTable>, SourceError> {
        self.blocking(|s| s.tables()).await
    }

    fn rows_table(rows: Vec<(String, String)>, truncated: bool) -> Table {
        Table {
            columns: vec!["key".into(), "value".into()],
            rows: rows
                .into_iter()
                .map(|(k, v)| vec![Value::Text(k), Value::Text(v)])
                .collect(),
            truncated,
        }
    }

    async fn run(&self, q: KvQuery) -> Result<Table, SourceError> {
        match q {
            KvQuery::Tables => {
                let tables = self.tables().await?;
                Ok(Table {
                    columns: vec![
                        "table".into(),
                        "key type".into(),
                        "value type".into(),
                        "entries".into(),
                    ],
                    rows: tables
                        .into_iter()
                        .map(|t| {
                            vec![
                                Value::Text(t.name),
                                Value::Text(t.key_type),
                                Value::Text(t.value_type),
                                t.len.map(|n| Value::Int(n as i64)).unwrap_or(Value::Null),
                            ]
                        })
                        .collect(),
                    truncated: false,
                })
            }
            KvQuery::Scan {
                table,
                prefix,
                limit,
            } => {
                let (rows, more) = self
                    .blocking(move |s| s.scan(&table, &prefix, limit))
                    .await?;
                Ok(Self::rows_table(rows, more))
            }
            KvQuery::Get { table, key } => {
                let shown = crate::kv::show_bytes(&key);
                let v = self.blocking(move |s| s.get(&table, &key)).await?;
                Ok(Self::rows_table(
                    v.map(|v| vec![(shown, v)]).unwrap_or_default(),
                    false,
                ))
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: KvStore> Source for KvSource<S> {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            display_name: self.database_node().label,
            family: SourceFamily::KeyValue,
            capabilities: Capabilities {
                read: true,
                write: false,
                watch: false,
                text_query: Some(TextDialect::Kv),
            },
            root: self.root_id(),
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Node(id) => {
                let key = self.key_of(id)?;
                if key.is_empty() {
                    return Ok(QueryResult::single(self.database_node()));
                }
                let name = key.strip_prefix("table:").ok_or(SourceError::NotFound)?;
                let t = self
                    .tables()
                    .await?
                    .into_iter()
                    .find(|t| t.name == name)
                    .ok_or(SourceError::NotFound)?;
                Ok(QueryResult::single(self.table_node(&t)))
            }
            Query::Children(id) | Query::Neighbours { node: id, .. } => {
                let key = self.key_of(id)?;
                let mut res = QueryResult::default();
                if key.is_empty() {
                    for t in self.tables().await? {
                        let n = self.table_node(&t);
                        res.edges.push(Edge::contains(&self.id, id, n.id));
                        res.nodes.push(n);
                    }
                }
                Ok(res)
            }
            Query::All { limit, .. } => {
                let db = self.database_node();
                let mut res = QueryResult::single(db.clone());
                for t in self.tables().await? {
                    let n = self.table_node(&t);
                    res.edges.push(Edge::contains(&self.id, db.id, n.id));
                    res.nodes.push(n);
                }
                if res.nodes.len() > limit {
                    res.nodes.truncate(limit);
                    res.truncated = true;
                }
                Ok(res)
            }
            Query::Text { dialect, text } => {
                if dialect != "kv" {
                    return Err(SourceError::Unsupported(format!(
                        "dialect {dialect}; this source speaks kv"
                    )));
                }
                let q = parse(&text).map_err(SourceError::Invalid)?;
                let table = self.run(q).await?;
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
            "a key/value table has no text; open it in the table editor".into(),
        ))
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "key/value sources are read-only in this milestone".into(),
        ))
    }
}
