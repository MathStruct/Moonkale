//! DuckDB source (Milestone 9): a `.duckdb` file, or a *data folder* —
//! CSV/TSV/Parquet files exposed as tables (views over `read_csv_auto` /
//! `read_parquet`) of an in-memory database. Same lifting as SQLite:
//! database → tables → columns; `Query::Text` runs read statements only.
//!
//! Ids: `duckdb:<absolute path>` (file) or `duckdb:<dir>/` (data folder);
//! nodes derived from `""`, `"table:<name>"`, `"column:<table>.<name>"`.
//! `Connection` is `Send` but not `Sync`: behind a `Mutex`, queries on
//! `spawn_blocking`.

use crate::text::{classify, Statement};
use duckdb::types::ValueRef;
use duckdb::{AccessMode, Config, Connection};
use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, Node, NodeId, NodeKind, Query, QueryResult, Source,
    SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction, Value,
    Version,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

pub const ROW_CAP: usize = 500;
/// Data files a folder exposes as tables.
pub const DATA_EXTENSIONS: &[&str] = &["csv", "tsv", "parquet"];
pub const DUCKDB_EXTENSIONS: &[&str] = &["duckdb", "ddb"];
const MAX_DATA_FILES: usize = 200;

pub use crate::{is_data_path, is_duckdb_path};

fn ext_in(path: &str, list: &[&str]) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| list.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub struct DuckDbSource {
    id: SourceId,
    path: PathBuf,
    /// Data folder mode: the files behind the views.
    files: Vec<PathBuf>,
    conn: Arc<Mutex<Connection>>,
    known: RwLock<HashMap<NodeId, String>>,
}

fn io(e: impl std::fmt::Display) -> SourceError {
    SourceError::Io(e.to_string())
}

impl DuckDbSource {
    /// Open a `.duckdb` file read-only.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        let config = Config::default()
            .access_mode(AccessMode::ReadOnly)
            .map_err(io)?;
        let conn = Connection::open_with_flags(&path, config).map_err(io)?;
        Ok(Self::new(
            format!("duckdb:{}", path.display()),
            path,
            Vec::new(),
            conn,
        ))
    }

    /// An in-memory database whose tables are the data files directly in
    /// `dir` (not recursive; first [`MAX_DATA_FILES`] by name).
    pub fn open_data_folder(dir: impl AsRef<Path>) -> Result<Self, SourceError> {
        let dir = std::fs::canonicalize(dir)?;
        if !dir.is_dir() {
            return Err(SourceError::Invalid(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_data_path(&p.to_string_lossy()))
            .collect();
        files.sort();
        files.truncate(MAX_DATA_FILES);
        if files.is_empty() {
            return Err(SourceError::Invalid(format!(
                "no CSV/TSV/Parquet files in {}",
                dir.display()
            )));
        }
        let conn = Connection::open_in_memory().map_err(io)?;
        let mut used: HashMap<String, usize> = HashMap::new();
        for f in &files {
            let stem = f
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut name: String = stem
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            if name.is_empty() || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                name = format!("t_{name}");
            }
            let n = used.entry(name.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                name = format!("{name}_{n}");
            }
            let path = f.to_string_lossy().replace('\'', "''");
            let reader = if ext_in(&f.to_string_lossy(), &["parquet"]) {
                format!("read_parquet('{path}')")
            } else {
                format!("read_csv_auto('{path}')")
            };
            let sql = format!(
                "CREATE VIEW \"{}\" AS SELECT * FROM {reader}",
                name.replace('"', "\"\"")
            );
            if let Err(e) = conn.execute_batch(&sql) {
                tracing::warn!("duckdb: {}: {e}", f.display());
            }
        }
        Ok(Self::new(
            format!("duckdb:{}/", dir.display()),
            dir,
            files,
            conn,
        ))
    }

    fn new(id: String, path: PathBuf, files: Vec<PathBuf>, conn: Connection) -> Self {
        let id = SourceId::new(id);
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Self {
            id,
            path,
            files,
            conn: Arc::new(Mutex::new(conn)),
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
            .unwrap_or_else(|| "duckdb".into());
        let label = if self.files.is_empty() {
            name
        } else {
            format!("{name}/ (data files)")
        };
        self.make("", NodeKind::Database, label)
    }

    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> duckdb::Result<T> + Send + 'static,
    ) -> Result<T, SourceError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let c = conn.lock().unwrap();
            f(&c)
        })
        .await
        .map_err(io)?
        .map_err(|e| SourceError::Invalid(e.to_string()))
    }

    async fn table_names(&self) -> Result<Vec<(String, String)>, SourceError> {
        self.blocking(|c| {
            let mut st = c.prepare("SELECT table_name, table_type FROM information_schema.tables WHERE table_schema = 'main' ORDER BY table_name")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect()
        })
        .await
    }

    async fn columns(&self, table: String) -> Result<Vec<(String, String)>, SourceError> {
        self.blocking(move |c| {
            let mut st = c.prepare("SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = 'main' AND table_name = ? ORDER BY ordinal_position")?;
            let rows = st.query_map([table], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect()
        })
        .await
    }

    async fn run_read(&self, sql: String) -> Result<Table, SourceError> {
        self.blocking(move |c| {
            let mut st = c.prepare(&sql)?;
            let mut it = st.query([])?;
            let mut columns: Vec<String> = Vec::new();
            let mut rows = Vec::new();
            let mut truncated = false;
            while let Some(r) = it.next()? {
                if columns.is_empty() {
                    columns = r.as_ref().column_names();
                }
                if rows.len() >= ROW_CAP {
                    truncated = true;
                    break;
                }
                let n = r.as_ref().column_count();
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    out.push(convert(r.get_ref(i)?));
                }
                rows.push(out);
            }
            if columns.is_empty() {
                columns = st.column_names();
            }
            Ok(Table {
                columns,
                rows,
                truncated,
            })
        })
        .await
    }

    fn table_label(name: &str, ty: &str) -> String {
        if ty.to_ascii_uppercase().contains("VIEW") {
            format!("{name} (view)")
        } else {
            name.to_string()
        }
    }

    async fn schema_graph(&self) -> Result<QueryResult, SourceError> {
        let db = self.database_node();
        let mut res = QueryResult::single(db.clone());
        for (name, ty) in self.table_names().await? {
            let t = self.make(
                &format!("table:{name}"),
                NodeKind::Table,
                Self::table_label(&name, &ty),
            );
            res.edges.push(Edge::contains(&self.id, db.id, t.id));
            for (col, cty) in self.columns(name.clone()).await? {
                let c = self.make(
                    &format!("column:{name}.{col}"),
                    NodeKind::Column,
                    format!("{col}: {cty}"),
                );
                res.edges.push(Edge::contains(&self.id, t.id, c.id));
                res.nodes.push(c);
            }
            res.nodes.push(t);
        }
        Ok(res)
    }
}

fn convert(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Boolean(b) => Value::Text(b.to_string()),
        ValueRef::TinyInt(i) => Value::Int(i as i64),
        ValueRef::SmallInt(i) => Value::Int(i as i64),
        ValueRef::Int(i) => Value::Int(i as i64),
        ValueRef::BigInt(i) => Value::Int(i),
        ValueRef::UTinyInt(i) => Value::Int(i as i64),
        ValueRef::USmallInt(i) => Value::Int(i as i64),
        ValueRef::UInt(i) => Value::Int(i as i64),
        ValueRef::UBigInt(i) => i64::try_from(i)
            .map(Value::Int)
            .unwrap_or_else(|_| Value::Text(i.to_string())),
        ValueRef::HugeInt(i) => Value::Text(i.to_string()),
        ValueRef::Float(f) => Value::Float(f as f64),
        ValueRef::Double(f) => Value::Float(f),
        ValueRef::Decimal(d) => Value::Text(d.to_string()),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Bytes {
            len: b.len() as u64,
        },
        other => Value::Text(format!("{other:?}")),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for DuckDbSource {
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
                            Self::table_label(&name, &ty),
                        );
                        res.edges.push(Edge::contains(&self.id, id, t.id));
                        res.nodes.push(t);
                    }
                } else if let Some(table) = key.strip_prefix("table:") {
                    for (col, cty) in self.columns(table.to_string()).await? {
                        let c = self.make(
                            &format!("column:{table}.{col}"),
                            NodeKind::Column,
                            format!("{col}: {cty}"),
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
            "DuckDB sources are read-only".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn data_folder_exposes_csv_as_tables_and_queries_them() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("people.csv"), "name,age\nAda,36\nBob,25\n").unwrap();
        std::fs::write(dir.path().join("2024-sales.tsv"), "item\tqty\npen\t3\n").unwrap();
        std::fs::write(dir.path().join("notes.md"), "not data").unwrap();
        let src = DuckDbSource::open_data_folder(dir.path()).unwrap();
        let root = src.query(Query::Children(src.root_id())).await.unwrap();
        let mut names: Vec<String> = root.nodes.iter().map(|n| n.label.clone()).collect();
        names.sort();
        assert_eq!(names, ["people (view)", "t_2024_sales (view)"]);
        let table = root
            .nodes
            .iter()
            .find(|n| n.label.starts_with("people"))
            .unwrap();
        let cols = src.query(Query::Children(table.id)).await.unwrap();
        assert_eq!(cols.nodes.len(), 2);
        let res = src
            .query(Query::Text {
                dialect: "sql".into(),
                text: "SELECT name, age FROM people WHERE age > 30".into(),
            })
            .await
            .unwrap();
        let t = res.table.unwrap();
        assert_eq!(t.columns, ["name", "age"]);
        assert_eq!(
            t.rows,
            vec![vec![Value::Text("Ada".into()), Value::Int(36)]]
        );
        let refused = src
            .query(Query::Text {
                dialect: "sql".into(),
                text: "DELETE FROM people".into(),
            })
            .await;
        assert!(matches!(refused, Err(SourceError::Unsupported(_))));
        assert_eq!(src.schema_graph().await.unwrap().nodes.len(), 1 + 2 + 4);
    }

    #[tokio::test]
    async fn duckdb_file_opens_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.duckdb");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE t(a INTEGER, b VARCHAR); INSERT INTO t VALUES (1, 'one'), (2, 'two');").unwrap();
        }
        let src = DuckDbSource::open(&path).unwrap();
        let res = src
            .query(Query::Text {
                dialect: "sql".into(),
                text: "SELECT count(*) FROM t".into(),
            })
            .await
            .unwrap();
        assert_eq!(res.table.unwrap().rows[0][0], Value::Int(2));
        assert!(
            is_duckdb_path("a/b.DuckDB") && is_data_path("x.parquet") && !is_data_path("x.json")
        );
    }
}
