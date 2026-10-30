//! DuckDB source (Milestone 9): a `.duckdb` file, or a *data folder* —
//! CSV/TSV/Parquet files exposed as tables (views over `read_csv_auto` /
//! `read_parquet`) of an in-memory database. Same lifting as SQLite:
//! database → tables → columns; `Query::Text` runs read statements only.
//!
//! Ids: `duckdb:<absolute path>` (file) or `duckdb:<dir>/` (data folder);
//! nodes derived from `""`, `"table:<name>"`, `"column:<table>.<name>"`.
//! `Connection` is `Send` but not `Sync`: behind a `Mutex`, queries on
//! `spawn_blocking`.

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

/// The most rows one text query returns. The table editor's grid is
/// virtualised, so the cap guards memory and transfer, not drawing.
pub const ROW_CAP: usize = 50_000;
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
    writable: bool,
    conn: Arc<Mutex<Option<Connection>>>,
    known: RwLock<HashMap<NodeId, String>>,
}

fn io(e: impl std::fmt::Display) -> SourceError {
    SourceError::Io(e.to_string())
}

impl DuckDbSource {
    /// Open a `.duckdb` file for typed row writes; text queries remain read-only.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        if path.to_str().is_none() {
            return Err(SourceError::Invalid(
                "database paths require an unambiguous UTF-8 source identity".into(),
            ));
        }
        let (conn, writable) = match Self::file_connection(&path, true) {
            Ok(c) => (c, true),
            Err(_) => (Self::file_connection(&path, false).map_err(io)?, false),
        };
        let mut source = Self::new(format!("duckdb:{}", path.display()), path, Vec::new(), conn);
        source.writable = writable;
        Ok(source)
    }

    // File handles last one operation: external applications may write while
    // this source is idle. The mutex serializes our own operations; a row batch
    // keeps this handle and its transaction through commit/rollback.
    fn file_connection(path: &Path, writable: bool) -> duckdb::Result<Connection> {
        let config = Config::default()
            .access_mode(if writable {
                AccessMode::ReadWrite
            } else {
                AccessMode::ReadOnly
            })
            .and_then(|c| c.enable_external_access(false))
            .and_then(|c| c.enable_autoload_extension(false))
            .and_then(|c| c.with("autoinstall_known_extensions", "false"))?;
        let conn = Connection::open_with_flags(path, config)?;
        conn.execute_batch("SET lock_configuration = true")?;
        Ok(conn)
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
        let config = Config::default()
            .enable_autoload_extension(false)
            .and_then(|c| c.with("autoinstall_known_extensions", "false"))
            .map_err(io)?;
        let conn = Connection::open_in_memory_with_flags(config).map_err(io)?;
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
        // The views read their files on every query, so file access stays —
        // but only to this folder, and nothing can switch that back (#2).
        let allowed = format!("{}/", dir.display()).replace('\'', "''");
        conn.execute_batch(&format!(
            "SET allowed_directories = ['{allowed}']; \
             SET enable_external_access = false; \
             SET lock_configuration = true;"
        ))
        .map_err(io)?;
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
        let writable = path.is_file();
        let memory = if path.is_file() { None } else { Some(conn) };
        Self {
            id,
            path,
            files,
            writable,
            conn: Arc::new(Mutex::new(memory)),
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
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap();
            let mut file;
            let c = if let Some(c) = guard.as_mut() {
                c
            } else {
                file = Self::file_connection(&path, false)?;
                &mut file
            };
            f(c)
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
        ValueRef::Date32(days) => moonkale_core::DateTimeCell::new(days as i64 * 86400, 0, None)
            .map(Value::DateTime)
            .unwrap_or_else(|| Value::Text(format!("Date32({days})"))),
        ValueRef::Timestamp(unit, value) => {
            use duckdb::types::TimeUnit;
            let per_second = match unit {
                TimeUnit::Second => 1,
                TimeUnit::Millisecond => 1000,
                TimeUnit::Microsecond => 1_000_000,
                TimeUnit::Nanosecond => 1_000_000_000,
            };
            moonkale_core::DateTimeCell::new(
                value.div_euclid(per_second),
                (value.rem_euclid(per_second) * (1_000_000_000 / per_second)) as u32,
                None,
            )
            .map(Value::DateTime)
            .unwrap_or_else(|| Value::Text(format!("Timestamp({unit:?}, {value})")))
        }
        ValueRef::List(items, i) => {
            use duckdb::types::ListType;
            let (values, start, end) = match items {
                ListType::Regular(a) => (
                    a.values(),
                    a.offsets()[i] as usize,
                    a.offsets()[i + 1] as usize,
                ),
                ListType::Large(a) => (
                    a.values(),
                    a.offsets()[i] as usize,
                    a.offsets()[i + 1] as usize,
                ),
            };
            numeric_array(values.as_ref(), start, end)
        }
        ValueRef::Array(a, i) => {
            let len = a.value_length() as usize;
            numeric_array(a.values().as_ref(), i * len, (i + 1) * len)
        }
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
        let path = self.path.clone();
        let writable = self.writable;
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap();
            let mut file;
            let c = if let Some(c) = guard.as_mut() {
                c
            } else {
                file = Self::file_connection(&path, writable).map_err(crate::rows::err)?;
                &mut file
            };
            let mut db = RowDb(c);
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
            "DuckDB sources are read-only".into(),
        ))
    }
}

/// Inspect Arrow's borrowed numeric carrier before allocating. Nested,
/// nullable, non-numeric and oversized lists remain a fixed text marker.
fn numeric_array(values: &dyn duckdb::arrow::array::Array, start: usize, end: usize) -> Value {
    use duckdb::arrow::array::{Float32Array, Float64Array, Int32Array, Int64Array};
    if end < start || end > values.len() || end - start > moonkale_core::NumericVector::MAX_LEN {
        return Value::Text("<list>".into());
    }
    let mut out = Vec::with_capacity(end - start);
    for i in start..end {
        if values.is_null(i) {
            return Value::Text("<list>".into());
        }
        let value = if let Some(a) = values.as_any().downcast_ref::<Float32Array>() {
            a.value(i) as f64
        } else if let Some(a) = values.as_any().downcast_ref::<Float64Array>() {
            a.value(i)
        } else if let Some(a) = values.as_any().downcast_ref::<Int32Array>() {
            a.value(i) as f64
        } else if let Some(a) = values.as_any().downcast_ref::<Int64Array>() {
            a.value(i) as f64
        } else {
            return Value::Text("<list>".into());
        };
        out.push(value);
    }
    moonkale_core::NumericVector::new(out)
        .map(Value::Vector)
        .unwrap_or_else(|| Value::Text("<list>".into()))
}

impl DuckDbSource {
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
        let path = self.path.clone();
        let writable = self.writable;
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap();
            let mut file;
            let c = if let Some(c) = guard.as_mut() {
                c
            } else {
                file = Self::file_connection(&path, writable).map_err(crate::rows::err)?;
                &mut file
            };
            let mut db = RowDb(c);
            tokio::runtime::Handle::current().block_on(crate::rows::batch(&mut db, batch, preview))
        })
        .await
        .map_err(crate::rows::err)?
    }
}

struct RowDb<'a>(&'a mut Connection);

fn bind_row_value(v: Value) -> Result<duckdb::types::Value, SourceError> {
    use duckdb::types::Value as D;
    Ok(match v {
        Value::Null => D::Null,
        Value::Int(n) => D::BigInt(n),
        Value::Float(n) if n.is_finite() => D::Double(n),
        Value::Bool(b) => D::Boolean(b),
        Value::Text(s) => D::Text(s),
        Value::DateTime(d) => D::Text(Value::DateTime(d).to_string()),
        Value::Vector(v) => D::Text(serde_json::to_string(v.as_slice()).map_err(crate::rows::err)?),
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
        let mut it = st
            .query(duckdb::params_from_iter(params))
            .map_err(crate::rows::err)?;
        let mut out = Vec::new();
        let mut bytes = 0usize;
        while let Some(r) = it.next().map_err(crate::rows::err)? {
            if out.len() > 4096 {
                return Err(SourceError::Invalid("row/catalog budget exceeded".into()));
            }
            let n = r.as_ref().column_count();
            let mut row = Vec::new();
            for i in 0..n {
                let value = match r.get_ref(i).map_err(crate::rows::err)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Text(s) if s.len() <= 512 * 1024 => {
                        Value::Text(String::from_utf8(s.to_vec()).map_err(crate::rows::err)?)
                    }
                    ValueRef::Boolean(b) => Value::Bool(b),
                    ValueRef::TinyInt(n) => Value::Int(n.into()),
                    ValueRef::SmallInt(n) => Value::Int(n.into()),
                    ValueRef::Int(n) => Value::Int(n.into()),
                    ValueRef::BigInt(n) => Value::Int(n),
                    ValueRef::Float(n) if n.is_finite() => Value::Float(n.into()),
                    ValueRef::Double(n) if n.is_finite() => Value::Float(n),
                    v @ (ValueRef::Date32(_) | ValueRef::Timestamp(_, _)) => {
                        let value = convert(v);
                        if !matches!(value, Value::DateTime(_)) {
                            return Err(crate::rows::err("date outside supported range"));
                        }
                        value
                    }
                    v @ (ValueRef::List(_, _) | ValueRef::Array(_, _)) => {
                        let value = convert(v);
                        if !matches!(value, Value::Vector(_)) {
                            return Err(crate::rows::err(
                                "list is not a bounded finite numeric vector",
                            ));
                        }
                        value
                    }
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
            .execute(sql, duckdb::params_from_iter(params))
            .map_err(crate::rows::err)?;
        Ok(())
    }
    async fn schema(&mut self, table: &str) -> Result<crate::rows::Schema, SourceError> {
        crate::rows::duck_schema(self, table).await
    }
    fn begin(&self) -> &'static str {
        "BEGIN TRANSACTION"
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
    /// #2: a SELECT must not read files beyond the source, in either mode.
    #[tokio::test]
    async fn no_file_access_beyond_the_source() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.csv");
        std::fs::write(&secret, "k,v\nkey,hunter2\n").unwrap();
        let attempts = |p: &str| {
            [
                format!("SELECT * FROM read_csv('{p}')"),
                format!("SELECT * FROM read_text('{p}')"),
                format!("SELECT * FROM glob('{p}')"),
                format!("WITH x AS (SELECT * FROM read_csv_auto('{p}')) SELECT * FROM x"),
                "SELECT * FROM read_csv('https://example.com/x.csv')".to_string(),
                "SET enable_external_access = true".to_string(),
            ]
        };
        let leaked = |r: &Result<QueryResult, SourceError>| {
            r.as_ref()
                .ok()
                .and_then(|r| r.table.as_ref())
                .is_some_and(|t| format!("{:?}", t.rows).contains("hunter2"))
        };

        // File mode.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.duckdb");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE t(a INTEGER)")
            .unwrap();
        let src = DuckDbSource::open(&path).unwrap();
        for sql in attempts(&secret.display().to_string()) {
            let r = src
                .query(Query::Text {
                    dialect: "sql".into(),
                    text: sql.clone(),
                })
                .await;
            assert!(r.is_err() && !leaked(&r), "file mode: {sql}: {r:?}");
        }

        // Folder mode: its own files still work, others do not.
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join("people.csv"), "name\nAda\n").unwrap();
        let src = DuckDbSource::open_data_folder(data.path()).unwrap();
        let own = src
            .query(Query::Text {
                dialect: "sql".into(),
                text: "SELECT count(*) FROM people".into(),
            })
            .await
            .unwrap();
        assert_eq!(own.table.unwrap().rows[0][0], Value::Int(1));
        for sql in attempts(&secret.display().to_string()) {
            let r = src
                .query(Query::Text {
                    dialect: "sql".into(),
                    text: sql.clone(),
                })
                .await;
            assert!(r.is_err() && !leaked(&r), "folder mode: {sql}: {r:?}");
        }
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;
    #[tokio::test]
    async fn dates_timestamps_lists_and_arrays_are_typed_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("seed.csv"), "x\n1\n").unwrap();
        let source = DuckDbSource::open_data_folder(dir.path()).unwrap();
        let table = source.run_read("SELECT DATE '2026-10-23', TIMESTAMP '1969-12-31 23:59:59.5', [1.0,2.0,3.0]::DOUBLE[], [1,2,3]::FLOAT[3], range(4097), [1,NULL], DATE 'infinity'".into()).await.unwrap();
        let row = &table.rows[0];
        assert!(matches!(row[0], Value::DateTime(_)));
        assert!(matches!(&row[1], Value::DateTime(d) if d.seconds() == -1));
        assert!(matches!(&row[2], Value::Vector(v) if v.as_slice() == [1.,2.,3.]));
        assert!(matches!(&row[3], Value::Vector(v) if v.as_slice() == [1.,2.,3.]));
        assert!(matches!(&row[4], Value::Text(_)));
        assert!(matches!(&row[5], Value::Text(_)));
        assert!(
            matches!(&row[6], Value::Text(_)),
            "out-of-range dates must not become NULL"
        );
    }
}
