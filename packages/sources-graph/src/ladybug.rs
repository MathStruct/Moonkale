//! LadybugDB source via the official `lbug` crate (LadybugDB is the
//! continuation of Kuzu: embedded, no server, Cypher).
//!
//! - Ids: `ladybug:<absolute dir>`; schema nodes derived from `""`
//!   (database), `"table:<name>"` (node or rel table),
//!   `"property:<table>.<name>"`; data nodes from
//!   `"v:<table_id>:<offset>"` (Ladybug's internal id).
//! - `Children(database)` → node tables; `Children(table)` → properties.
//! - `Query::All` → the schema graph: rel tables become edges between their
//!   node tables (`EdgeKind::Custom(<rel name>)`).
//! - `Query::Text { dialect: "cypher" }` runs any statement (the database is
//!   opened read-only), capped at [`ROW_CAP`] rows. The result is a `Table`
//!   of `Value`s **and**, when a column yields `NODE`/`REL`, the matching
//!   `Vertex` nodes and edges so the Graph panel can draw the answer.
//! - `lbug` is a sync C++ binding; every call runs on `spawn_blocking`.
//!
//! Build notes: by default `lbug` downloads a prebuilt static `liblbug`;
//! if unavailable it compiles the C++ library with cmake. `LBUG_SHARED` /
//! `LBUG_LIBRARY_DIR` / `LBUG_INCLUDE_DIR` link against a system install;
//! `LBUG_BUILD_FROM_SOURCE` forces a source build.
//!
//! See vault: `research/Database Backends.md`.

use lbug::{Connection, Database, SystemConfig};
use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, EdgeKind, Node, NodeId, NodeKind, Query, QueryResult,
    Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction,
    Value, Version,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const ROW_CAP: usize = 500;

/// A Ladybug database is a directory (or, since 0.11, a single file) whose
/// name ends in one of these, or a directory holding a `catalog.kz`.
pub fn looks_like_database(path: &Path) -> bool {
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e, "lbug" | "kuzu" | "kz"));
    ext_ok || path.join("catalog.kz").exists()
}

pub struct LadybugSource {
    id: SourceId,
    path: PathBuf,
    db: Arc<Database>,
    known: RwLock<HashMap<NodeId, String>>,
}

#[derive(Clone, Debug)]
struct TableInfo {
    name: String,
    /// `NODE` or `REL`.
    kind: String,
}

#[derive(Clone, Debug)]
struct PropertyInfo {
    name: String,
    ty: String,
    primary: bool,
}

impl LadybugSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        let mut cfg = SystemConfig::default();
        cfg = cfg.read_only(true);
        let db = Database::new(&path, cfg).map_err(|e| SourceError::Io(e.to_string()))?;
        let id = SourceId::new(format!("ladybug:{}", path.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Ok(Self {
            id,
            path,
            db: Arc::new(db),
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
        f: impl FnOnce(&Connection) -> Result<T, lbug::Error> + Send + 'static,
    ) -> Result<T, SourceError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = Connection::new(&db)?;
            f(&conn)
        })
        .await
        .map_err(|e| SourceError::Io(e.to_string()))?
        .map_err(|e| SourceError::Invalid(e.to_string()))
    }

    async fn tables(&self) -> Result<Vec<TableInfo>, SourceError> {
        self.blocking(|c| {
            let mut r = c.query("CALL show_tables() RETURN name, type")?;
            let mut out = Vec::new();
            for row in &mut r {
                if let [lbug::Value::String(name), lbug::Value::String(kind)] = row.as_slice() {
                    out.push(TableInfo {
                        name: name.clone(),
                        kind: kind.clone(),
                    });
                }
            }
            out.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(out)
        })
        .await
    }

    async fn properties(&self, table: String) -> Result<Vec<PropertyInfo>, SourceError> {
        self.blocking(move |c| {
            let mut r = c.query(&format!(
                "CALL table_info('{}') RETURN name, type, `primary key`",
                table.replace('\'', "''")
            ))?;
            let mut out = Vec::new();
            for row in &mut r {
                if let [lbug::Value::String(name), lbug::Value::String(ty), lbug::Value::Bool(pk)] =
                    row.as_slice()
                {
                    out.push(PropertyInfo {
                        name: name.clone(),
                        ty: ty.clone(),
                        primary: *pk,
                    });
                }
            }
            Ok(out)
        })
        .await
    }

    /// Endpoints of a rel table: `(from node table, to node table)` pairs.
    async fn rel_endpoints(&self, table: String) -> Result<Vec<(String, String)>, SourceError> {
        self.blocking(move |c| {
            let mut r = c.query(&format!(
                "CALL show_connection('{}') RETURN *",
                table.replace('\'', "''")
            ))?;
            let mut out = Vec::new();
            for row in &mut r {
                if let (Some(lbug::Value::String(f)), Some(lbug::Value::String(t))) =
                    (row.first(), row.get(1))
                {
                    out.push((f.clone(), t.clone()));
                }
            }
            Ok(out)
        })
        .await
    }

    fn property_node(&self, table: &str, p: &PropertyInfo) -> Node {
        let label = if p.primary {
            format!("{}: {} (pk)", p.name, p.ty)
        } else {
            format!("{}: {}", p.name, p.ty)
        };
        self.make(
            &format!("property:{table}.{}", p.name),
            NodeKind::Column,
            label,
        )
    }

    /// The schema as a graph: database → node tables → properties, with rel
    /// tables drawn as edges between their endpoint tables.
    async fn schema_graph(&self) -> Result<QueryResult, SourceError> {
        let db = self.database_node();
        let mut res = QueryResult::single(db.clone());
        let tables = self.tables().await?;
        for t in tables.iter().filter(|t| t.kind == "NODE") {
            let node = self.make(
                &format!("table:{}", t.name),
                NodeKind::Table,
                t.name.clone(),
            );
            res.edges.push(Edge::contains(&self.id, db.id, node.id));
            for p in self.properties(t.name.clone()).await? {
                let c = self.property_node(&t.name, &p);
                res.edges.push(Edge::contains(&self.id, node.id, c.id));
                res.nodes.push(c);
            }
            res.nodes.push(node);
        }
        for t in tables.iter().filter(|t| t.kind == "REL") {
            for (from, to) in self.rel_endpoints(t.name.clone()).await? {
                res.edges.push(Edge {
                    source: self.id.clone(),
                    from: self.node_id(&format!("table:{from}")),
                    to: self.node_id(&format!("table:{to}")),
                    kind: EdgeKind::Custom(t.name.clone()),
                });
            }
        }
        Ok(res)
    }

    /// Run Cypher; rows become `Value`s, node/rel columns become graph
    /// nodes/edges as well.
    async fn run_cypher(&self, text: String) -> Result<QueryResult, SourceError> {
        let id = self.id.clone();
        let (table, verts, rels) = self
            .blocking(move |c| {
                let mut r = c.query(&text)?;
                let columns = r.get_column_names();
                let n = columns.len();
                let mut rows = Vec::new();
                let mut verts: Vec<lbug::NodeVal> = Vec::new();
                let mut rels: Vec<lbug::RelVal> = Vec::new();
                let mut truncated = false;
                for row in &mut r {
                    if rows.len() >= ROW_CAP {
                        truncated = true;
                        break;
                    }
                    let mut out = Vec::with_capacity(n);
                    for v in row {
                        match &v {
                            lbug::Value::Node(nv) => verts.push(nv.clone()),
                            lbug::Value::Rel(rv) => rels.push(rv.clone()),
                            lbug::Value::RecursiveRel { nodes, rels: rs } => {
                                verts.extend(nodes.iter().cloned());
                                rels.extend(rs.iter().cloned());
                            }
                            _ => {}
                        }
                        out.push(convert(&v));
                    }
                    rows.push(out);
                }
                Ok((
                    Table {
                        columns,
                        rows,
                        truncated,
                    },
                    verts,
                    rels,
                ))
            })
            .await?;
        let _ = id;
        let mut res = QueryResult {
            truncated: table.truncated,
            table: Some(table),
            ..Default::default()
        };
        let mut seen = std::collections::HashSet::new();
        for v in &verts {
            let key = vertex_key(v.get_node_id());
            if !seen.insert(key.clone()) {
                continue;
            }
            res.nodes
                .push(self.make(&key, NodeKind::Vertex, vertex_label(v)));
        }
        for r in &rels {
            let from = self.node_id(&vertex_key(r.get_src_node()));
            let to = self.node_id(&vertex_key(r.get_dst_node()));
            res.edges.push(Edge {
                source: self.id.clone(),
                from,
                to,
                kind: EdgeKind::Custom(r.get_label_name().clone()),
            });
        }
        Ok(res)
    }
}

fn vertex_key(id: &lbug::InternalID) -> String {
    format!("v:{}:{}", id.table_id, id.offset)
}

/// `Label(pk or first string property)`.
fn vertex_label(v: &lbug::NodeVal) -> String {
    let props = v.get_properties();
    let first = props
        .iter()
        .find(|(_, val)| matches!(val, lbug::Value::String(_)))
        .or_else(|| props.first())
        .map(|(_, val)| val.to_string());
    match first {
        Some(s) => format!("{}({s})", v.get_label_name()),
        None => v.get_label_name().clone(),
    }
}

fn convert(v: &lbug::Value) -> Value {
    use lbug::Value as L;
    match v {
        L::Null(_) => Value::Null,
        L::Bool(b) => Value::Bool(*b),
        L::Int64(i) => Value::Int(*i),
        L::Int32(i) => Value::Int(*i as i64),
        L::Int16(i) => Value::Int(*i as i64),
        L::Int8(i) => Value::Int(*i as i64),
        L::UInt64(i) => i64::try_from(*i)
            .map(Value::Int)
            .unwrap_or_else(|_| Value::Text(i.to_string())),
        L::UInt32(i) => Value::Int(*i as i64),
        L::UInt16(i) => Value::Int(*i as i64),
        L::UInt8(i) => Value::Int(*i as i64),
        L::Double(f) => Value::Float(*f),
        L::Float(f) => Value::Float(*f as f64),
        L::String(s) => Value::Text(s.clone()),
        L::Blob(b) => Value::Bytes {
            len: b.len() as u64,
        },
        L::Node(n) => Value::Text(vertex_label(n)),
        L::Rel(r) => Value::Text(format!("-[{}]->", r.get_label_name())),
        other => Value::Text(other.to_string()),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for LadybugSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            display_name: self.database_node().label,
            family: SourceFamily::Graph,
            capabilities: Capabilities {
                read: true,
                write: false,
                watch: false,
                text_query: Some(TextDialect::Cypher),
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
                } else if let Some(p) = key.strip_prefix("property:") {
                    self.make(
                        &key,
                        NodeKind::Column,
                        p.rsplit('.').next().unwrap_or(p).to_string(),
                    )
                } else if key.starts_with("v:") {
                    self.make(&key, NodeKind::Vertex, key.clone())
                } else {
                    return Err(SourceError::NotFound);
                };
                Ok(QueryResult::single(node))
            }
            Query::Children(id) | Query::Neighbours { node: id, .. } => {
                let key = self.key_of(id)?;
                let mut res = QueryResult::default();
                if key.is_empty() {
                    for t in self.tables().await? {
                        let label = if t.kind == "REL" {
                            format!("{} (rel)", t.name)
                        } else {
                            t.name.clone()
                        };
                        let n = self.make(&format!("table:{}", t.name), NodeKind::Table, label);
                        res.edges.push(Edge::contains(&self.id, id, n.id));
                        res.nodes.push(n);
                    }
                } else if let Some(table) = key.strip_prefix("table:") {
                    for p in self.properties(table.to_string()).await? {
                        let c = self.property_node(table, &p);
                        res.edges.push(Edge::contains(&self.id, id, c.id));
                        res.nodes.push(c);
                    }
                } else if let Some(rest) = key.strip_prefix("v:") {
                    // One hop around a vertex.
                    let (tid, off) = rest.split_once(':').ok_or(SourceError::NotFound)?;
                    let cypher = format!(
                        "MATCH (a)-[r]-(b) WHERE id(a) = internal_id({tid}, {off}) RETURN a, r, b LIMIT {ROW_CAP}"
                    );
                    let mut r = self.run_cypher(cypher).await?;
                    r.table = None;
                    return Ok(r);
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
                if dialect != "cypher" {
                    return Err(SourceError::Unsupported(format!(
                        "dialect {dialect}; this source speaks cypher"
                    )));
                }
                self.run_cypher(text).await
            }
        }
    }

    async fn fetch_text(&self, _node: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::Unsupported(
            "graph nodes have no text; query the database".into(),
        ))
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "Ladybug sources are opened read-only in this milestone".into(),
        ))
    }
}
