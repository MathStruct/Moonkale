//! HelixDB, embedded (Milestone 17, spec 002): graph + vector in one engine
//! (SlateDB underneath), opened in-process with the `helix-db` crate's
//! `embedded` feature — a git dependency, the engine is not on crates.io.
//!
//! - On disk a Helix store is an object-store root directory (`*.helix`)
//!   holding one directory per logical database (`main/wal`, `…/compactions`);
//!   the source opens the only one, or `main` when there are several.
//! - Opened with `Client::open_reader`: read-only, a writer keeps its lock.
//! - Ids: `helix:<absolute path>`; nodes derived from `""` (database),
//!   `"label:<L>"` / `"edge:<L>"` (the labels, as `Table` nodes, so a click
//!   opens the table editor), `"v:<Label>:<id>"` (a vertex — the same shape as
//!   LadybugDB's, so the graph panel colours by label).
//! - `Query::All` → vertices + edges (`EdgeKind::Custom(label)`), capped.
//! - `Query::Text { dialect: "helix" }` → `nodes [<label>] [limit <n>]`,
//!   `edges [<label>] [limit <n>]`: a table of `$id`, `$label` (`$from`,
//!   `$to`) and the properties, plus the vertices/edges for *Show in Graph*.
//! - The server mode (HTTP `POST /v2/query`) is the second backend of this
//!   file still to come; the web build could use it directly.

use helix_db::dsl::prelude::*;
use helix_db::{Client, HelixDbSource, QueryRequest};
use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, EdgeKind, Node, NodeId, NodeKind, Query, QueryResult,
    Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Table, TextDialect, Transaction,
    Value, Version,
};
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Most vertices/edges one query returns.
pub const ROW_CAP: usize = 2000;
/// How many vertices/edges are read to list the labels.
const LABEL_SCAN: usize = 20_000;

pub struct HelixSource {
    id: SourceId,
    path: PathBuf,
    database: String,
    client: Client,
    known: RwLock<HashMap<NodeId, String>>,
}

fn err(e: impl std::fmt::Display) -> SourceError {
    SourceError::Invalid(e.to_string())
}

/// The logical database inside a Helix root: the only subdirectory, or
/// `main` when there are several.
fn pick_database(root: &Path) -> Result<String, SourceError> {
    let mut dirs: Vec<String> = std::fs::read_dir(root)?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    dirs.sort();
    match dirs.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(SourceError::Invalid(format!(
            "{} holds no Helix database",
            root.display()
        ))),
        many if many.iter().any(|d| d == "main") => Ok("main".into()),
        many => Err(SourceError::Invalid(format!(
            "{} holds several Helix databases ({}); none is called main",
            root.display(),
            many.join(", ")
        ))),
    }
}

/// `nodes [<label>] [limit <n>]` / `edges [<label>] [limit <n>]`.
#[derive(Debug, PartialEq)]
struct HelixQuery {
    edges: bool,
    label: Option<String>,
    limit: usize,
}

fn parse(text: &str) -> Result<HelixQuery, String> {
    let usage = "helix: nodes [<label>] [limit <n>] | edges [<label>] [limit <n>]";
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = text.trim().chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                if let Some(e) = chars.next() {
                    cur.push(e)
                }
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    let edges = match words.first().map(|w| w.to_ascii_lowercase()).as_deref() {
        Some("nodes") => false,
        Some("edges") => true,
        _ => return Err(usage.into()),
    };
    let mut label = None;
    let mut limit = 200;
    let mut i = 1;
    while i < words.len() {
        if words[i].eq_ignore_ascii_case("limit") {
            limit = words
                .get(i + 1)
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| usage.to_string())?;
            i += 2;
        } else if label.is_none() {
            label = Some(words[i].clone());
            i += 1;
        } else {
            return Err(usage.into());
        }
    }
    Ok(HelixQuery {
        edges,
        label,
        limit: limit.clamp(1, ROW_CAP),
    })
}

fn json_value(v: &Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or_else(|| Value::Text(n.to_string())),
        Json::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

fn str_of(o: &Json, k: &str) -> String {
    match o.get(k) {
        Some(Json::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

impl HelixSource {
    /// Open a Helix root directory (read-only).
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = std::fs::canonicalize(path)?;
        let database = pick_database(&path)?;
        let client = Client::open_reader(HelixDbSource::Disk {
            root: path.clone(),
            database: database.clone(),
        })
        .await
        .map_err(|e| SourceError::Io(e.to_string()))?;
        let id = SourceId::new(format!("helix:{}", path.display()));
        let mut known = HashMap::new();
        known.insert(NodeId::derive(&id, ""), String::new());
        Ok(Self {
            id,
            path,
            database,
            client,
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
            .unwrap_or_else(|| "helix".into());
        let label = if self.database == "main" {
            name
        } else {
            format!("{name} · {}", self.database)
        };
        self.make("", NodeKind::Database, label)
    }

    async fn run(&self, request: QueryRequest) -> Result<Json, SourceError> {
        self.client.query::<Json>(request).send().await.map_err(err)
    }

    /// Vertices (`$id`, `$label`, properties), optionally of one label.
    async fn vertices(&self, label: Option<&str>, limit: usize) -> Result<Vec<Json>, SourceError> {
        let t = match label {
            Some(l) => g().n_with_label(l),
            None => g().n(NodeRef::all()),
        };
        let q = read_batch()
            .var_as("v", t.limit(limit).value_map(None::<Vec<String>>))
            .returning(["v"]);
        let r = self.run(QueryRequest::read(q)).await?;
        Ok(r.get("v")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// Edges (`$id`, `$label`, `$from`, `$to`, properties).
    async fn edges(&self, label: Option<&str>, limit: usize) -> Result<Vec<Json>, SourceError> {
        let t = match label {
            Some(l) => g().e_with_label(l),
            None => g().e(EdgeRef::all()),
        };
        let q = read_batch()
            .var_as("e", t.limit(limit).edge_properties())
            .returning(["e"]);
        let r = self.run(QueryRequest::read(q)).await?;
        Ok(r.get("e")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// Node labels and edge labels with their counts (over the first
    /// [`LABEL_SCAN`] of each).
    async fn labels(&self) -> Result<(BTreeMap<String, u64>, BTreeMap<String, u64>), SourceError> {
        let mut nodes = BTreeMap::new();
        for v in self.vertices(None, LABEL_SCAN).await? {
            *nodes.entry(str_of(&v, "$label")).or_insert(0) += 1;
        }
        let mut edges = BTreeMap::new();
        for e in self.edges(None, LABEL_SCAN).await? {
            *edges.entry(str_of(&e, "$label")).or_insert(0) += 1;
        }
        Ok((nodes, edges))
    }

    fn vertex_key(v: &Json) -> String {
        format!("v:{}:{}", str_of(v, "$label"), str_of(v, "$id"))
    }

    fn vertex_node(&self, v: &Json) -> Node {
        let name = ["name", "title", "label", "id"]
            .iter()
            .find_map(|k| v.get(*k).and_then(Json::as_str).map(str::to_string))
            .unwrap_or_else(|| format!("{} {}", str_of(v, "$label"), str_of(v, "$id")));
        self.make(&Self::vertex_key(v), NodeKind::Vertex, name)
    }

    /// A graph from vertices and edges; edges whose ends are not among the
    /// vertices bring a stub vertex (id and label unknown → `?`).
    fn graph(&self, vertices: &[Json], edges: &[Json]) -> QueryResult {
        let mut res = QueryResult::default();
        let mut by_id: HashMap<String, NodeId> = HashMap::new();
        for v in vertices {
            let n = self.vertex_node(v);
            by_id.insert(str_of(v, "$id"), n.id);
            res.nodes.push(n);
        }
        for e in edges {
            let mut end = |id: String, res: &mut QueryResult| {
                *by_id.entry(id.clone()).or_insert_with(|| {
                    let n = self.make(&format!("v:?:{id}"), NodeKind::Vertex, format!("#{id}"));
                    let nid = n.id;
                    res.nodes.push(n);
                    nid
                })
            };
            let from = end(str_of(e, "$from"), &mut res);
            let to = end(str_of(e, "$to"), &mut res);
            res.edges.push(Edge {
                props: Default::default(),
                from,
                to,
                kind: EdgeKind::Custom(str_of(e, "$label")),
                source: self.id.clone(),
            });
        }
        res
    }

    /// Rows for the table editor: `$id`, `$label` (edges: `$from`, `$to`),
    /// then every property that occurs, sorted.
    fn table(rows: &[Json], edges: bool, truncated: bool) -> Table {
        let mut columns: Vec<String> = vec!["$id".into(), "$label".into()];
        if edges {
            columns.push("$from".into());
            columns.push("$to".into());
        }
        let props: BTreeSet<String> = rows
            .iter()
            .filter_map(Json::as_object)
            .flat_map(|o| o.keys().filter(|k| !k.starts_with('$')).cloned())
            .collect();
        columns.extend(props);
        Table {
            rows: rows
                .iter()
                .map(|r| {
                    columns
                        .iter()
                        .map(|c| r.get(c).map(json_value).unwrap_or(Value::Null))
                        .collect()
                })
                .collect(),
            columns,
            truncated,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for HelixSource {
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
                text_query: Some(TextDialect::Helix),
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
                } else if let Some(l) = key.strip_prefix("label:") {
                    self.make(&key, NodeKind::Table, l.to_string())
                } else if let Some(l) = key.strip_prefix("edge:") {
                    self.make(&key, NodeKind::Table, format!("{l} (edge)"))
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
                    let (nodes, edges) = self.labels().await?;
                    for (l, n) in nodes {
                        let t =
                            self.make(&format!("label:{l}"), NodeKind::Table, format!("{l} · {n}"));
                        res.edges.push(Edge::contains(&self.id, id, t.id));
                        res.nodes.push(t);
                    }
                    for (l, n) in edges {
                        let t = self.make(
                            &format!("edge:{l}"),
                            NodeKind::Table,
                            format!("{l} (edge) · {n}"),
                        );
                        res.edges.push(Edge::contains(&self.id, id, t.id));
                        res.nodes.push(t);
                    }
                }
                Ok(res)
            }
            Query::All { limit, .. } => {
                let limit = limit.min(ROW_CAP);
                let vertices = self.vertices(None, limit).await?;
                let edges = self.edges(None, limit).await?;
                let mut res = self.graph(&vertices, &edges);
                res.truncated = vertices.len() >= limit || edges.len() >= limit;
                Ok(res)
            }
            Query::Text { dialect, text } => {
                if dialect != "helix" {
                    return Err(SourceError::Unsupported(format!(
                        "dialect {dialect}; this source speaks helix"
                    )));
                }
                let q = parse(&text).map_err(SourceError::Invalid)?;
                let label = q.label.as_deref();
                let (mut res, rows) = if q.edges {
                    let e = self.edges(label, q.limit).await?;
                    (self.graph(&[], &e), e)
                } else {
                    let v = self.vertices(label, q.limit).await?;
                    (self.graph(&v, &[]), v)
                };
                let truncated = rows.len() >= q.limit;
                res.table = Some(Self::table(&rows, q.edges, truncated));
                res.truncated = truncated;
                Ok(res)
            }
        }
    }

    async fn fetch_text(&self, _node: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::Unsupported(
            "graph nodes have no text; open the label".into(),
        ))
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "Helix sources are read-only in this milestone".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nodes_and_edges() {
        assert_eq!(
            parse("nodes"),
            Ok(HelixQuery {
                edges: false,
                label: None,
                limit: 200
            })
        );
        assert_eq!(
            parse(r#"edges "LIVES IN" limit 5"#),
            Ok(HelixQuery {
                edges: true,
                label: Some("LIVES IN".into()),
                limit: 5
            })
        );
        assert!(parse("match (n)").is_err());
        assert!(parse("nodes a b").is_err());
    }
}
