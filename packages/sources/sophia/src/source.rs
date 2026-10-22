//! The `Source` over `sophia-interface` (Sophia SPEC-004): a translation of
//! types. Sophia's calls are synchronous, so each runs on the blocking pool.
use moonkale_core::{
    Applied, Capabilities, Changes, ContentRef, Direction, Edge, EdgeKind, Node, NodeId, NodeKind,
    Op, OpResult, Query, QueryResult, Risk, Source, SourceDescriptor, SourceError, SourceFamily,
    SourceId, SourceTruth, Table, TextDialect, Transaction, Value, Version,
};
use sophia_interface::{
    Direction as SDirection, EdgeView, EditError, Interface, Key, NodeView, Prop, Subgraph,
};
use sophia_store::query::{QueryLimits, Value as SValue, VIEWS};
use sophia_store::sqlite::Sqlite;
use sophia_store::StoreError;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Every Moonkale user reads through one profile row set for now; a per-user
/// reader comes with Moonkale's user ids (SPEC-004 §4).
const READER: &str = "moonkale";
/// Bounds for answers Sophia does not cap itself.
const NEIGHBOUR_LIMIT: usize = 10_000;
const MAX_DEPTH: u32 = 64;
const SQL_ROWS: usize = 10_000;
/// Remembered ids before the map is cleared (the root and settings stay).
const MAX_KEYS: usize = 1_000_000;
/// `changes_since` waits this long for news, polling the log (v0 has no push).
const LONG_POLL: Duration = Duration::from_secs(25);
const POLL_EVERY: Duration = Duration::from_millis(500);
const FEED_BATCH: usize = 256;
/// The folder of Sophia's SQL views under the root, so the table panel can
/// open them like a database's tables. Moonkale's, not Sophia's: no store
/// key starts with `moonkale:` or `table:`.
const VIEWS_DIR: &str = "moonkale:views";
const VIEW_PREFIX: &str = "table:";

pub struct SophiaSource {
    id: SourceId,
    root: NodeId,
    display_name: String,
    iface: Arc<Interface<Sqlite>>,
    /// Moonkale hands back ids; Sophia wants keys. Filled by every answer.
    keys: Mutex<HashMap<NodeId, Key>>,
}

impl SophiaSource {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, SourceError> {
        Self::open_as(path, READER).await
    }

    /// Open for a given reader, whose rendering profile Sophia applies.
    pub async fn open_as(
        path: impl Into<PathBuf>,
        reader: impl Into<String>,
    ) -> Result<Self, SourceError> {
        let (path, reader) = (path.into(), reader.into());
        tokio::task::spawn_blocking(move || Self::load(&path, reader))
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?
    }

    fn load(path: &Path, reader: String) -> Result<Self, SourceError> {
        if !path.is_file() {
            return Err(SourceError::NotFound);
        }
        let backend = Sqlite::open(path).map_err(store_error)?;
        Self::with_interface(
            SourceId::new(format!("sophia:{}", path.display())),
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Sophia".into()),
            Interface::open(backend, reader).map_err(store_error)?,
        )
    }

    /// An in-memory store with SPEC-004 §9's sample, for tests and demos.
    pub fn sample() -> Result<Self, SourceError> {
        let iface = Interface::open(Sqlite::open_in_memory().map_err(store_error)?, READER)
            .map_err(store_error)?;
        iface.load_sample().map_err(store_error)?;
        Self::with_interface(
            SourceId::new("sophia:sample"),
            "Sophia sample".into(),
            iface,
        )
    }

    fn with_interface(
        id: SourceId,
        display_name: String,
        iface: Interface<Sqlite>,
    ) -> Result<Self, SourceError> {
        let root_key = iface.root();
        let source = Self {
            root: key_id(&id, &root_key),
            id,
            display_name,
            iface: Arc::new(iface),
            keys: Mutex::new(HashMap::new()),
        };
        source.remember(&root_key);
        source.remember(&Key::settings());
        source.remember_views();
        Ok(source)
    }

    fn remember(&self, key: &Key) -> NodeId {
        let id = key_id(&self.id, key);
        let mut keys = self.keys.lock().unwrap_or_else(|e| e.into_inner());
        if keys.len() >= MAX_KEYS {
            keys.clear();
            keys.insert(self.root, self.iface.root());
            keys.insert(key_id(&self.id, &Key::settings()), Key::settings());
            for k in view_keys() {
                keys.insert(key_id(&self.id, &k), k);
            }
        }
        keys.insert(id, key.clone());
        id
    }

    fn remember_views(&self) {
        for k in view_keys() {
            self.remember(&k);
        }
    }

    /// The views folder or one view, which Sophia doesn't know.
    fn view_node(&self, key: &Key) -> Option<Node> {
        let Key::Derived(s) = key else { return None };
        let (kind, label) = if s == VIEWS_DIR {
            (NodeKind::Directory, "views")
        } else {
            let v = s.strip_prefix(VIEW_PREFIX)?;
            (NodeKind::Table, *VIEWS.iter().find(|n| **n == v)?)
        };
        Some(Node {
            id: key_id(&self.id, key),
            source: self.id.clone(),
            kind,
            label: label.into(),
            native_key: s.clone(),
            content: None,
            version: Version(0),
            props: Default::default(),
        })
    }

    fn contains(&self, from: NodeId, to: NodeId) -> Edge {
        Edge {
            source: self.id.clone(),
            from,
            to,
            kind: EdgeKind::Contains,
            props: Default::default(),
        }
    }

    /// The views folder's subgraph: the folder, its views, and the root's
    /// edge to it.
    fn views_graph(&self) -> QueryResult {
        let keys = view_keys();
        let nodes: Vec<Node> = keys.iter().filter_map(|k| self.view_node(k)).collect();
        let dir = nodes[0].id;
        let mut edges = vec![self.contains(self.root, dir)];
        edges.extend(nodes[1..].iter().map(|n| self.contains(dir, n.id)));
        QueryResult {
            nodes,
            edges,
            table: None,
            truncated: false,
        }
    }

    fn key(&self, id: NodeId) -> Result<Key, SourceError> {
        let keys = self.keys.lock().unwrap_or_else(|e| e.into_inner());
        keys.get(&id).cloned().ok_or(SourceError::NotFound)
    }

    /// Run a synchronous interface call on the blocking pool.
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Interface<Sqlite>) -> Result<T, SourceError> + Send + 'static,
    ) -> Result<T, SourceError> {
        let iface = self.iface.clone();
        tokio::task::spawn_blocking(move || f(&iface))
            .await
            .map_err(|e| SourceError::Io(e.to_string()))?
    }

    fn node(&self, v: &NodeView) -> Node {
        let id = self.remember(&v.key);
        let texty = matches!(v.kind.as_str(), "Name" | "Definition" | "sophia:Settings");
        Node {
            id,
            source: self.id.clone(),
            kind: match v.kind.as_str() {
                "Directory" => NodeKind::Directory,
                k => NodeKind::Custom(k.into()),
            },
            label: v.label.clone(),
            native_key: native_key(&v.key),
            content: texty.then(|| ContentRef::Text {
                len: 0,
                lang: (v.kind != "sophia:Settings").then(|| "sophialang".into()),
            }),
            version: Version(0),
            props: v.props.iter().map(|(k, p)| (k.clone(), value(p))).collect(),
        }
    }

    fn edge(&self, e: &EdgeView, from: NodeId, to: NodeId) -> Edge {
        Edge {
            source: self.id.clone(),
            from,
            to,
            kind: match e.kind.as_str() {
                "Contains" => EdgeKind::Contains,
                "References" => EdgeKind::References,
                "Calls" => EdgeKind::Calls,
                k => EdgeKind::Custom(k.into()),
            },
            props: e.props.iter().map(|(k, p)| (k.clone(), value(p))).collect(),
        }
    }

    /// The subgraph, keeping only edges whose ends are in it.
    fn result(&self, g: &Subgraph, kinds: Option<&[NodeKind]>) -> QueryResult {
        // Translate/remember each node once. Edges reuse those ids instead of
        // hashing and locking the same endpoint keys twice per edge (P-224).
        // The map contains only retained nodes, so kind-filtered/dangling edges
        // are rejected before copying their properties.
        let mut ids = HashMap::with_capacity(g.nodes.len());
        let nodes: Vec<Node> = g
            .nodes
            .iter()
            .filter_map(|v| {
                let node = self.node(v);
                if kinds.is_some_and(|k| !k.contains(&node.kind)) {
                    return None;
                }
                ids.insert(&v.key, node.id);
                Some(node)
            })
            .collect();
        let edges = g
            .edges
            .iter()
            .filter_map(|e| Some(self.edge(e, *ids.get(&e.from)?, *ids.get(&e.to)?)))
            .collect();
        QueryResult {
            nodes,
            edges,
            table: None,
            truncated: g.truncated,
        }
    }

    async fn op(&self, op: Op) -> Result<(NodeId, Version), SourceError> {
        let target = match &op {
            Op::WriteText { node, .. } | Op::Rename { node, .. } | Op::Delete { node } => *node,
            Op::CreateText { parent, .. } | Op::CreateDir { parent, .. } => *parent,
        };
        if self.key(target).is_ok_and(|k| self.view_node(&k).is_some()) {
            return Err(SourceError::Unsupported(
                "Sophia's views are read-only".into(),
            ));
        }
        match op {
            Op::WriteText {
                node,
                expected,
                patch,
            } => {
                let key = self.key(node)?;
                let version = self
                    .call(move |i| {
                        let (text, found) = i.fetch_text(&key).map_err(store_error)?;
                        if found != expected.0 {
                            return Err(SourceError::Conflict {
                                expected,
                                actual: Version(found),
                            });
                        }
                        let new = patch.apply(&text)?;
                        i.write_text(&key, expected.0, &new)
                            .map(|w| w.version)
                            .map_err(|e| edit_error(e, &new))
                    })
                    .await?;
                Ok((node, Version(version)))
            }
            Op::CreateText { parent, name, text } => {
                let parent = self.key(parent)?;
                let (key, version) = self
                    .call(move |i| {
                        i.create_text(&parent, &name, &text)
                            .map(|(k, w)| (k, w.version))
                            .map_err(|e| edit_error(e, &text))
                    })
                    .await?;
                Ok((self.remember(&key), Version(version)))
            }
            Op::CreateDir { parent, name } => {
                let parent = self.key(parent)?;
                let key = self
                    .call(move |i| i.create_dir(&parent, &name).map_err(|e| edit_error(e, "")))
                    .await?;
                Ok((self.remember(&key), Version(0)))
            }
            Op::Rename { node, to } => {
                let key = self.key(node)?;
                let key = self
                    .call(move |i| i.rename(&key, &to).map_err(|e| edit_error(e, "")))
                    .await?;
                Ok((self.remember(&key), Version(0)))
            }
            Op::Delete { node } => {
                let key = self.key(node)?;
                self.call(move |i| i.delete(&key).map_err(|e| edit_error(e, "")))
                    .await?;
                Ok((node, Version(0)))
            }
        }
    }

    /// One read of the log after `since`: the position to continue from,
    /// the native keys that changed, and whether to re-read everything.
    fn feed(i: &Interface<Sqlite>, since: u64) -> Result<(u64, Vec<String>, bool), SourceError> {
        let (changes, next, more) = i.changes_since(since, FEED_BATCH).map_err(store_error)?;
        let mut seen = BTreeSet::new();
        let mut paths = Vec::new();
        let mut reset = more;
        for c in &changes {
            reset |= c.reprint_all;
            for k in &c.keys {
                let k = native_key(k);
                if seen.insert(k.clone()) {
                    paths.push(k);
                }
            }
        }
        Ok((next.max(since), paths, reset))
    }
}

/// The views folder first, then one key per view.
fn view_keys() -> Vec<Key> {
    std::iter::once(Key::Derived(VIEWS_DIR.into()))
        .chain(
            VIEWS
                .iter()
                .map(|v| Key::Derived(format!("{VIEW_PREFIX}{v}"))),
        )
        .collect()
}

/// `Key::Content(d)` → `from_content(d)`; `Key::Derived(s)` → `derive(source, s)`.
fn key_id(source: &SourceId, key: &Key) -> NodeId {
    match key {
        Key::Content(d) => NodeId::from_content(d.as_bytes()),
        Key::Derived(s) => NodeId::derive(source, s),
    }
}

/// The derived key as is (`ns:a.b`, `name:a.b.f`, `settings`), or `#<hex>`
/// for content. The change feed reports these, and `derive(source, key)` of
/// a gone name is its old id, which is what the workspace falls back to.
fn native_key(key: &Key) -> String {
    match key {
        Key::Content(d) => format!("#{}", d.to_hex()),
        Key::Derived(s) => s.clone(),
    }
}

fn parse_native_key(text: &str) -> Result<Key, SourceError> {
    match text.strip_prefix('#') {
        Some(hex) => hex
            .parse()
            .map(Key::Content)
            .map_err(|_| SourceError::Invalid(format!("not a Sophia digest: {hex}"))),
        None => Ok(Key::Derived(text.into())),
    }
}

fn value(p: &Prop) -> Value {
    match p {
        Prop::Text(s) => Value::Text(s.clone()),
        Prop::Int(i) => Value::Int(*i),
        Prop::Bool(b) => Value::Bool(*b),
    }
}

fn store_error(e: StoreError) -> SourceError {
    match e {
        StoreError::NotFound(_) | StoreError::NoSuchName(_) => SourceError::NotFound,
        StoreError::Conflict {
            expected, found, ..
        } => SourceError::Conflict {
            expected: Version(expected),
            actual: Version(found),
        },
        StoreError::NameExists(_) | StoreError::InvalidPath { .. } => {
            SourceError::Invalid(e.to_string())
        }
        StoreError::QueryRefused(m) => SourceError::Unsupported(format!("query refused: {m}")),
        e => SourceError::Io(e.to_string()),
    }
}

/// Parse errors become one `Invalid` with a `line:col: message` per line;
/// `text` is what was parsed, so the spans can be placed.
fn edit_error(e: EditError, text: &str) -> SourceError {
    match e {
        EditError::Diagnostics(ds) => SourceError::Invalid(
            ds.iter()
                .map(|d| {
                    let (line, col) = d.line_col(text);
                    format!("{line}:{col}: {}", d.message)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        EditError::Conflict { expected, found } => SourceError::Conflict {
            expected: Version(expected),
            actual: Version(found),
        },
        EditError::Refused(m) => SourceError::Unsupported(m),
        EditError::Store(e) => store_error(e),
    }
}

fn table(t: &sophia_store::query::Table) -> Table {
    Table {
        columns: t.columns.clone(),
        rows: t
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|v| match v {
                        SValue::Null => Value::Null,
                        SValue::Int(i) => Value::Int(*i),
                        SValue::Real(f) => Value::Float(*f),
                        SValue::Text(s) => Value::Text(s.clone()),
                        SValue::Blob(b) => Value::Bytes {
                            len: b.len() as u64,
                        },
                    })
                    .collect()
            })
            .collect(),
        truncated: t.truncated,
    }
}

#[moonkale_core::async_trait]
impl Source for SophiaSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            truth: SourceTruth::Store,
            id: self.id(),
            display_name: self.display_name.clone(),
            family: SourceFamily::Graph,
            capabilities: Capabilities {
                read: true,
                write: true,
                watch: true,
                text_query: Some(TextDialect::Sql),
                extra_text_query: Some(TextDialect::SophiaQuery),
            },
            root: self.root,
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Node(id) => {
                let key = self.key(id)?;
                if let Some(n) = self.view_node(&key) {
                    return Ok(QueryResult::single(n));
                }
                let v = self
                    .call(move |i| i.node(&key).map_err(store_error))
                    .await?;
                v.map(|v| QueryResult::single(self.node(&v)))
                    .ok_or(SourceError::NotFound)
            }
            Query::Children(id) => {
                let key = self.key(id)?;
                match self.view_node(&key) {
                    Some(n) if n.kind == NodeKind::Directory => {
                        let mut r = self.views_graph();
                        r.nodes.remove(0);
                        r.edges.remove(0);
                        return Ok(r);
                    }
                    Some(_) => return Ok(QueryResult::default()),
                    None => {}
                }
                let g = self
                    .call(move |i| i.children(&key).map_err(store_error))
                    .await?;
                let mut r = self.result(&g, None);
                if id == self.root {
                    let v = self.views_graph();
                    r.nodes.push(v.nodes[0].clone());
                    r.edges.push(v.edges[0].clone());
                }
                Ok(r)
            }
            Query::Neighbours {
                node,
                depth,
                direction,
            } => {
                let key = self.key(node)?;
                if self.view_node(&key).is_some() {
                    return Ok(self.views_graph());
                }
                let dir = match direction {
                    Direction::Out => SDirection::Out,
                    Direction::In => SDirection::In,
                    Direction::Both => SDirection::Both,
                };
                let g = self
                    .call(move |i| {
                        i.neighbours(&key, depth.min(MAX_DEPTH), dir, NEIGHBOUR_LIMIT)
                            .map_err(store_error)
                    })
                    .await?;
                let mut r = self.result(&g, None);
                r.truncated |= depth > MAX_DEPTH;
                Ok(r)
            }
            Query::All { limit, kinds } => {
                let start = std::time::Instant::now();
                let g = self
                    .call(move |i| i.all(limit).map_err(store_error))
                    .await?;
                let interface_ms = start.elapsed().as_secs_f64() * 1000.0;
                let start = std::time::Instant::now();
                let result = self.result(&g, kinds.as_deref());
                if std::env::var_os("MOONKALE_GRAPH_PROFILE").is_some() {
                    eprintln!(
                        "graph source profile: interface_ms={interface_ms} adapter_ms={}",
                        start.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Ok(result)
            }
            // What the workspace asks for each path the change feed named.
            Query::Text { dialect, text } if dialect == "path" => {
                let key = parse_native_key(&text)?;
                if let Some(n) = self.view_node(&key) {
                    return Ok(QueryResult::single(n));
                }
                let v = self
                    .call(move |i| i.node(&key).map_err(store_error))
                    .await?;
                v.map(|v| QueryResult::single(self.node(&v)))
                    .ok_or(SourceError::NotFound)
            }
            Query::Text { dialect, text } if dialect == "sql" => {
                let limits = QueryLimits {
                    max_rows: SQL_ROWS,
                    ..Default::default()
                };
                let (t, g) = self
                    .call(move |i| i.sql(&text, limits).map_err(store_error))
                    .await?;
                let mut r = self.result(&g, None);
                r.truncated |= t.truncated;
                r.table = Some(table(&t));
                Ok(r)
            }
            // Sophia's own language (SPEC-007 §8), read-only on every backend.
            Query::Text { dialect, text } if dialect == "query" => {
                let (t, g) = self
                    .call(move |i| i.query(&text, Default::default()).map_err(store_error))
                    .await?;
                let mut r = self.result(&g, None);
                r.truncated |= t.truncated;
                r.table = Some(table(&t));
                Ok(r)
            }
            Query::Text { dialect, .. } => Err(SourceError::Unsupported(format!(
                "Sophia speaks sql and query, not {dialect}"
            ))),
        }
    }

    /// `query` is Sophia's read-only language: Sophia refuses anything else.
    fn classify(&self, dialect: &str, text: &str) -> Risk {
        match dialect {
            "query" => Risk::Read,
            _ => moonkale_core::source::risk::classify(dialect, text),
        }
    }

    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        let key = self.key(node)?;
        if self.view_node(&key).is_some() {
            return Err(SourceError::Unsupported("a view has no text".into()));
        }
        let (text, v) = self
            .call(move |i| i.fetch_text(&key).map_err(store_error))
            .await?;
        Ok((text, Version(v)))
    }

    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        let mut results = Vec::with_capacity(tx.ops.len());
        for op in tx.ops {
            let node = match &op {
                Op::WriteText { node, .. } | Op::Rename { node, .. } | Op::Delete { node } => *node,
                Op::CreateText { parent, .. } | Op::CreateDir { parent, .. } => *parent,
            };
            results.push(match self.op(op).await {
                Ok((node, version)) => OpResult::Ok { node, version },
                Err(error) => OpResult::Refused { node, error },
            });
        }
        Ok(Applied { results })
    }

    async fn changes_since(&self, since: u64) -> Result<Option<Changes>, SourceError> {
        // The first call only learns the position; the workspace ignores
        // what it lists. An empty log waits like any other poll.
        let mut since = since;
        if since == 0 {
            // Only the tail position is needed, not every historical change.
            // Replaying a generated store's log held SQLite's read lock ahead
            // of the initial graph query for seconds (P-224).
            let start = Instant::now();
            let end = self
                .call(|i| i.store().last_seq().map_err(store_error))
                .await?;
            if std::env::var_os("MOONKALE_GRAPH_PROFILE").is_some() {
                eprintln!(
                    "graph source profile: watch_baseline_ms={}",
                    start.elapsed().as_secs_f64() * 1000.0
                );
            }
            if end > 0 {
                return Ok(Some(Changes {
                    seq: end,
                    paths: Vec::new(),
                    reset: false,
                }));
            }
            since = end;
        }
        let start = Instant::now();
        loop {
            let (seq, paths, reset) = self.call(move |i| Self::feed(i, since)).await?;
            if !paths.is_empty() || reset || start.elapsed() >= LONG_POLL {
                return Ok(Some(Changes { seq, paths, reset }));
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[tokio::test]
    async fn initial_watch_starts_at_the_tail_without_replaying_history() {
        let source = SophiaSource::sample().unwrap();
        let tail = source.iface.store().last_seq().unwrap();
        assert!(tail > 0);
        let initial = source.changes_since(0).await.unwrap().unwrap();
        assert_eq!(initial.seq, tail);
        assert!(initial.paths.is_empty());
        assert!(!initial.reset);
    }
}
