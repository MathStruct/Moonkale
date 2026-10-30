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
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
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

/// A print snapshot may change when another name moves, even if this
/// definition's Sophia token does not. Patches address this exact text.
#[derive(Clone)]
struct EditBase {
    node: NodeId,
    version: Version,
    token: u64,
    text: Arc<str>,
}
const MAX_EDIT_BASES: usize = 128;
const MAX_EDIT_BASE_BYTES: usize = 32 * 1024 * 1024;
const MAX_EDIT_BASE_TEXT: usize = 2 * 1024 * 1024;

fn editable_key(key: &Key) -> bool {
    matches!(key, Key::Derived(s) if s.starts_with("name:") || s == "settings")
}

fn text_version(node: NodeId, token: u64, text: &str) -> Version {
    // An opaque equality token, never an authorisation or cryptographic identity.
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hash);
    token.hash(&mut hash);
    text.hash(&mut hash);
    Version(hash.finish())
}

pub struct SophiaSource {
    id: SourceId,
    root: NodeId,
    display_name: String,
    iface: Arc<Interface<Sqlite>>,
    derived_path: Option<PathBuf>,
    folder_root: Option<PathBuf>,
    stale: Arc<Mutex<BTreeSet<String>>>,
    /// Moonkale hands back ids; Sophia wants keys. Filled by every answer.
    keys: Mutex<HashMap<NodeId, Key>>,
    edit_bases: Mutex<VecDeque<EditBase>>,
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
        let iface = Interface::open(backend, reader).map_err(store_error)?;
        let derived = iface.store().is_derived().map_err(store_error)?;
        let mut source = Self::with_interface(
            SourceId::new(format!("sophia:{}", path.display())),
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Sophia".into()),
            iface,
        )?;
        if derived {
            source.derived_path = Some(path.to_path_buf());
            // Keep no connection to the derived file between calls: Windows
            // also needs to be able to delete/rebuild it while this source lives.
            source.iface = Arc::new(Interface::open_read_only(
                Sqlite::open_in_memory().map_err(store_error)?,
                READER,
            ));
            if path.file_name().is_some_and(|n| n == "code.sophia")
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|n| n == ".moonkale")
            {
                source.folder_root = path.parent().and_then(Path::parent).map(Path::to_path_buf);
            }
        }
        Ok(source)
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
            derived_path: None,
            folder_root: None,
            stale: Arc::new(Mutex::new(BTreeSet::new())),
            keys: Mutex::new(HashMap::new()),
            edit_bases: Mutex::new(VecDeque::new()),
        };
        source.remember(&root_key);
        source.remember(&Key::settings());
        source.remember_views();
        Ok(source)
    }

    fn remember_text(&self, node: NodeId, token: u64, text: &str) -> Version {
        let version = text_version(node, token, text);
        if text.len() > MAX_EDIT_BASE_TEXT {
            return version;
        }
        let mut bases = self.edit_bases.lock().unwrap_or_else(|e| e.into_inner());
        if bases.iter().any(|b| b.node == node && b.version == version) {
            return version;
        }
        let mut bytes: usize = bases.iter().map(|b| b.text.len()).sum();
        while bases.len() >= MAX_EDIT_BASES || bytes + text.len() > MAX_EDIT_BASE_BYTES {
            let Some(old) = bases.pop_front() else {
                break;
            };
            bytes -= old.text.len();
        }
        bases.push_back(EditBase {
            node,
            version,
            token,
            text: Arc::from(text),
        });
        version
    }

    /// Files for which a read found a different source id (or no current file).
    /// A later successful fresh read clears that file's stale marker.
    pub fn stale_files(&self) -> Vec<String> {
        self.stale
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
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
        let path = self.derived_path.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(path) = path {
                if !path.is_file() {
                    return Err(SourceError::NotFound);
                }
                let backend = Sqlite::open(path).map_err(store_error)?;
                let current = Interface::open_read_only(backend, READER);
                f(&current)
            } else {
                f(&iface)
            }
        })
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
            props: {
                let mut props: moonkale_core::graph::Properties =
                    v.props.iter().map(|(k, p)| (k.clone(), value(p))).collect();
                if self.derived_path.is_some() {
                    props.insert("derived".into(), Value::Bool(true));
                    props.insert("stale".into(), Value::Bool(!self.stale_files().is_empty()));
                }
                props
            },
        }
    }

    fn edge(&self, e: &EdgeView, from: NodeId, to: NodeId) -> Edge {
        Edge {
            source: self.id.clone(),
            from,
            to,
            kind: match e.kind.as_str() {
                "Contains" => EdgeKind::Contains,
                "Defines" => EdgeKind::Defines,
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

    async fn op(&self, op: Op) -> Result<(NodeId, Version, Vec<String>), SourceError> {
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
                if !editable_key(&key) {
                    return Err(SourceError::Unsupported(
                        "Sophia content is read-only; edit its name".into(),
                    ));
                }
                let base = self
                    .edit_bases
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .find(|b| b.node == node && b.version == expected)
                    .cloned();
                let (token, printed, warnings) = self
                    .call(move |i| {
                        let (current, found) = i.fetch_text(&key).map_err(store_error)?;
                        let actual = text_version(node, found, &current);
                        let original = match &base {
                            Some(base) if base.token == found => base.text.as_ref(),
                            None if actual == expected => current.as_str(),
                            _ => return Err(SourceError::Conflict { expected, actual }),
                        };
                        let new = patch.apply(original)?;
                        let written = i.write_text(&key, found, &new).map_err(|e| {
                            let error = edit_error(e, &new);
                            if matches!(error, SourceError::Conflict { .. }) {
                                match i.fetch_text(&key) {
                                    Ok((text, token)) => SourceError::Conflict {
                                        expected,
                                        actual: text_version(node, token, &text),
                                    },
                                    Err(_) => error,
                                }
                            } else {
                                error
                            }
                        })?;
                        // Return the canonical print for this write. If an external
                        // writer already changed it again, acknowledge only ours;
                        // the next save will conflict against the newer raw token.
                        let printed = match i.fetch_text(&key) {
                            Ok((text, token)) if token == written.version => text,
                            _ => new,
                        };
                        Ok((
                            written.version,
                            printed,
                            written.warnings.into_iter().map(|d| d.message).collect(),
                        ))
                    })
                    .await?;
                let version = self.remember_text(node, token, &printed);
                Ok((node, version, warnings))
            }
            Op::CreateText { parent, name, text } => {
                let parent = self.key(parent)?;
                let (key, token, printed, warnings) = self
                    .call(move |i| {
                        let (key, written) = i
                            .create_text(&parent, &name, &text)
                            .map_err(|e| edit_error(e, &text))?;
                        let printed = match i.fetch_text(&key) {
                            Ok((printed, token)) if token == written.version => printed,
                            _ => text,
                        };
                        Ok((
                            key,
                            written.version,
                            printed,
                            written.warnings.into_iter().map(|d| d.message).collect(),
                        ))
                    })
                    .await?;
                let node = self.remember(&key);
                let version = self.remember_text(node, token, &printed);
                Ok((node, version, warnings))
            }
            Op::CreateDir { parent, name } => {
                let parent = self.key(parent)?;
                let key = self
                    .call(move |i| i.create_dir(&parent, &name).map_err(|e| edit_error(e, "")))
                    .await?;
                Ok((self.remember(&key), Version(0), Vec::new()))
            }
            Op::Rename { node, to } => {
                let key = self.key(node)?;
                let key = self
                    .call(move |i| i.rename(&key, &to).map_err(|e| edit_error(e, "")))
                    .await?;
                Ok((self.remember(&key), Version(0), Vec::new()))
            }
            Op::Delete { node } => {
                let key = self.key(node)?;
                self.call(move |i| i.delete(&key).map_err(|e| edit_error(e, "")))
                    .await?;
                Ok((node, Version(0), Vec::new()))
            }
        }
    }

    /// One read of the log after `since`: the position to continue from,
    /// the native keys that changed, and whether to re-read everything.
    fn feed(i: &Interface<Sqlite>, since: u64) -> Result<(u64, Vec<String>, bool), SourceError> {
        let end = i.store().last_seq().map_err(store_error)?;
        if end < since {
            return Ok((end, Vec::new(), true));
        }
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
            truth: if self.derived_path.is_some() {
                SourceTruth::Files
            } else {
                SourceTruth::Store
            },
            id: self.id(),
            display_name: self.display_name.clone(),
            family: SourceFamily::Graph,
            capabilities: Capabilities {
                read: true,
                write: self.derived_path.is_none(),
                watch: true,
                text_query: Some(TextDialect::Sql),
                extra_text_query: Some(TextDialect::SophiaQuery),
                atomic_text: false,
                atomic_rows: false,
                table_query: Default::default(),
            },
            root: self.root,
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        match query {
            Query::Text { dialect, text }
                if dialect == "definition-locations" && self.derived_path.is_some() =>
            {
                if text.len() > 8192 {
                    return Err(SourceError::Invalid("location query limit".into()));
                }
                let Key::Content(id) = parse_native_key(&text)? else {
                    return Ok(QueryResult::default());
                };
                let rows = self
                    .call(move |i| {
                        let mut ids = i.store().methods(&id).map_err(store_error)?;
                        if ids.is_empty() {
                            ids.push(id);
                        }
                        if ids.len() > 1 {
                            return Ok(Vec::new());
                        }
                        let files = i.store().files().map_err(store_error)?;
                        if files.len() > 5000 {
                            return Err(SourceError::Invalid("location file limit".into()));
                        }
                        let mut rows = Vec::new();
                        let mut work = 0usize;
                        for path in files {
                            let Some(m) = i.store().file_manifest(&path).map_err(store_error)?
                            else {
                                continue;
                            };
                            work = work.saturating_add(m.spans.len());
                            if work > NEIGHBOUR_LIMIT {
                                return Err(SourceError::Invalid("location span limit".into()));
                            }
                            let mut input = None;
                            for (record, start, end) in m.spans {
                                if ids.contains(&record) {
                                    if input.is_none() {
                                        let Some(source) = m.source else { continue };
                                        let Some(source) =
                                            i.store().record(&source).map_err(store_error)?
                                        else {
                                            continue;
                                        };
                                        if source.bytes.len() > 512 * 1024 {
                                            return Err(SourceError::Invalid(
                                                "location source limit".into(),
                                            ));
                                        }
                                        let text =
                                            std::str::from_utf8(&source.bytes).map_err(|_| {
                                                SourceError::Invalid(
                                                    "invalid location source".into(),
                                                )
                                            })?;
                                        input = Some(Value::Text(
                                            NodeId::derive(&SourceId::new("index-input-v1"), text)
                                                .to_string(),
                                        ));
                                    }
                                    rows.push(vec![
                                        Value::Text(path.clone()),
                                        Value::Int(start as i64),
                                        Value::Int(end as i64),
                                        input.clone().unwrap(),
                                    ]);
                                    if rows.len() == 2 {
                                        return Ok(rows);
                                    }
                                }
                            }
                        }
                        Ok(rows)
                    })
                    .await?;
                Ok(QueryResult {
                    table: Some(Table {
                        columns: vec![
                            "file".into(),
                            "start_byte".into(),
                            "end_byte".into(),
                            "input".into(),
                        ],
                        rows,
                        truncated: false,
                    }),
                    ..Default::default()
                })
            }
            Query::Text { dialect, text }
                if dialect == "folder-neighbours" && self.derived_path.is_some() =>
            {
                if text.len() > 8192 {
                    return Err(SourceError::Invalid("folder query limit".into()));
                }
                let (path, depth, direction): (String, u32, Direction) =
                    serde_json::from_str(&text)
                        .map_err(|_| SourceError::Invalid("invalid folder query".into()))?;
                let dir = match direction {
                    Direction::In => SDirection::In,
                    Direction::Out => SDirection::Out,
                    Direction::Both => SDirection::Both,
                };
                let g = self
                    .call(move |i| {
                        let manifest = i.store().file_manifest(&path).map_err(store_error)?;
                        let mut out = Subgraph {
                            nodes: Vec::new(),
                            edges: Vec::new(),
                            truncated: false,
                        };
                        let mut seen = BTreeSet::new();
                        let mut seen_edges = BTreeSet::new();
                        let mut work = 0usize;
                        if let Some(manifest) = manifest {
                            for (id, _, _) in manifest.spans {
                                if work >= NEIGHBOUR_LIMIT {
                                    out.truncated = true;
                                    break;
                                }
                                let graph = i
                                    .neighbours(
                                        &Key::Content(id),
                                        depth.min(MAX_DEPTH),
                                        dir,
                                        NEIGHBOUR_LIMIT - work,
                                    )
                                    .map_err(store_error)?;
                                work = work
                                    .saturating_add(graph.nodes.len())
                                    .saturating_add(graph.edges.len());
                                for node in graph.nodes {
                                    if seen.insert(format!("{:?}", node.key)) {
                                        out.nodes.push(node);
                                    }
                                }
                                for edge in graph.edges {
                                    if out.edges.len() >= NEIGHBOUR_LIMIT {
                                        out.truncated = true;
                                        break;
                                    }
                                    let key = (
                                        format!("{:?}", edge.from),
                                        format!("{:?}", edge.to),
                                        edge.kind.clone(),
                                    );
                                    if seen_edges.insert(key) {
                                        out.edges.push(edge);
                                    }
                                }
                                out.truncated |= graph.truncated || work > NEIGHBOUR_LIMIT;
                            }
                        }
                        out.truncated |= depth > MAX_DEPTH;
                        Ok(out)
                    })
                    .await?;
                Ok(self.result(&g, None))
            }
            Query::Coarse { level, region } => {
                use moonkale_core::source::coarse::{project, MAX_LEVEL};
                if level > MAX_LEVEL {
                    return Err(SourceError::Invalid("coarse level limit".into()));
                }
                let g = self.call(coarse_metadata).await?;
                let mut snapshot = self.result(&g, None);
                // Names alias content-addressed definitions. Assign each shared
                // definition to its smallest stable name id for navigation only;
                // retain every Names edge as a real relation in the projection.
                let owned: std::collections::HashSet<_> = snapshot
                    .edges
                    .iter()
                    .filter(|e| matches!(e.kind, EdgeKind::Contains | EdgeKind::Defines))
                    .map(|e| e.to)
                    .collect();
                let mut owners = std::collections::BTreeMap::<NodeId, NodeId>::new();
                for edge in &snapshot.edges {
                    if edge.kind == EdgeKind::Custom("Names".into()) && !owned.contains(&edge.to) {
                        owners
                            .entry(edge.to)
                            .and_modify(|id| *id = (*id).min(edge.from))
                            .or_insert(edge.from);
                    }
                }
                snapshot.edges.extend(
                    owners
                        .into_iter()
                        .map(|(definition, name)| self.contains(name, definition)),
                );
                project(snapshot, level, region)
            }
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
                        let mut g = i
                            .neighbours(&key, depth.min(MAX_DEPTH), dir, NEIGHBOUR_LIMIT)
                            .map_err(store_error)?;
                        if depth > 0 {
                            generic_calls(i, &mut g, NEIGHBOUR_LIMIT)?;
                        }
                        Ok(g)
                    })
                    .await?;
                let mut r = self.result(&g, None);
                r.truncated |= depth > MAX_DEPTH;
                Ok(r)
            }
            Query::All { limit, kinds } => {
                let start = std::time::Instant::now();
                let g = self
                    .call(move |i| {
                        let start = Instant::now();
                        let mut g = i.all(limit).map_err(store_error)?;
                        let all_ms = start.elapsed().as_secs_f64() * 1000.0;
                        let start = Instant::now();
                        generic_calls(i, &mut g, limit)?;
                        if std::env::var_os("MOONKALE_GRAPH_PROFILE").is_some() {
                            eprintln!(
                                "graph source profile: all_ms={all_ms} generic_calls_ms={}",
                                start.elapsed().as_secs_f64() * 1000.0
                            );
                        }
                        Ok(g)
                    })
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
        if let Some(root) = self.folder_root.clone() {
            let stale = self.stale.clone();
            let key_for_source = key.clone();
            let sources = self
                .call(move |i| {
                    if let Some(ts) = i.text_source(&key_for_source).map_err(store_error)? {
                        return Ok(vec![ts]);
                    }
                    let generic = match &key_for_source {
                        Key::Derived(s) if s.starts_with("name:") => i
                            .store()
                            .resolve(&s[5..])
                            .map_err(store_error)?
                            .map(|n| n.target),
                        Key::Content(d) => Some(sophia_store::Target::Nominal(*d)),
                        _ => None,
                    };
                    let mut sources = Vec::new();
                    if let Some(sophia_store::Target::Nominal(id)) = generic {
                        for member in i.store().read(|tx| tx.relation(&id)).map_err(store_error)? {
                            if let Some(ts) = i
                                .text_source(&Key::Content(member.member))
                                .map_err(store_error)?
                            {
                                sources.push(ts);
                            }
                        }
                    }
                    Ok(sources)
                })
                .await?;
            if !sources.is_empty() {
                return tokio::task::spawn_blocking(move || {
                    let mut bodies = Vec::new();
                    let mut revision = 0u64;
                    for ts in sources {
                        let result = read_current(&root, &ts.path);
                        let mut markers = stale.lock().unwrap_or_else(|e| e.into_inner());
                        let text = match result {
                            Ok(text) => text,
                            Err(e) => {
                                markers.insert(ts.path);
                                return Err(e);
                            }
                        };
                        let id = sophia_store::folder::source_record(&ts.path, &text).id;
                        revision ^= u64::from_le_bytes(id.as_bytes()[..8].try_into().unwrap());
                        if id != ts.source {
                            markers.insert(ts.path);
                            // Old spans cannot safely address newer text.
                            bodies.push(text);
                        } else {
                            markers.remove(&ts.path);
                            let body =
                                text.get(ts.start as usize..ts.end as usize)
                                    .ok_or_else(|| {
                                        SourceError::Invalid("stored span is invalid".into())
                                    })?;
                            bodies.push(body.to_string());
                        }
                    }
                    Ok((bodies.join("\n"), Version(revision)))
                })
                .await
                .map_err(|e| SourceError::Io(e.to_string()))?;
            }
        }
        let editable = editable_key(&key);
        let (text, v) = self
            .call(move |i| i.fetch_text(&key).map_err(store_error))
            .await?;
        let version = if self.derived_path.is_none() && editable {
            self.remember_text(node, v, &text)
        } else {
            Version(v)
        };
        Ok((text, version))
    }

    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        if self.derived_path.is_some() {
            return Err(SourceError::Unsupported(
                "derived store: edit the files".into(),
            ));
        }
        let mut results = Vec::with_capacity(tx.ops.len());
        for op in tx.ops {
            let node = match &op {
                Op::WriteText { node, .. } | Op::Rename { node, .. } | Op::Delete { node } => *node,
                Op::CreateText { parent, .. } | Op::CreateDir { parent, .. } => *parent,
            };
            results.push(match self.op(op).await {
                Ok((node, version, warnings)) => OpResult::Ok {
                    node,
                    version,
                    warnings,
                },
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

/// Coarse navigation reads metadata only. Interface::all also decodes/traverses
/// every definition to infer Calls; a sufficiently deep stored term can exhaust
/// its worker stack (P-229). References are indexed metadata and need no body.
fn coarse_metadata(interface: &Interface<Sqlite>) -> Result<Subgraph, SourceError> {
    use moonkale_core::source::coarse::{MAX_INPUT_EDGES, MAX_INPUT_NODES};
    let limit = || SourceError::Invalid("coarse Sophia metadata limit".into());
    let start = interface
        .store()
        .read(|tx| tx.last_seq())
        .map_err(store_error)?;
    let root = interface.root();
    let mut nodes = HashMap::new();
    if let Some(node) = interface.node(&root).map_err(store_error)? {
        nodes.insert(root.clone(), node);
    }
    let mut pending = std::collections::VecDeque::from([root]);
    let mut visited = std::collections::HashSet::new();
    let mut edges = Vec::new();
    while let Some(key) = pending.pop_front() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let children = interface.children(&key).map_err(store_error)?;
        if nodes.len() + children.nodes.len() > MAX_INPUT_NODES
            || edges.len() + children.edges.len() > MAX_INPUT_EDGES
        {
            return Err(limit());
        }
        for node in children.nodes {
            if node.kind == "Directory" {
                pending.push_back(node.key.clone());
            }
            nodes.insert(node.key.clone(), node);
        }
        edges.extend(children.edges);
    }
    // One metadata transaction supplies names, definition ids and indexed refs.
    // No definition_of/Term traversal or body parsing is performed.
    let (names, records, references) = interface
        .store()
        .read(|tx| {
            let names = tx.names_under("", MAX_INPUT_NODES + 1)?;
            let records = tx.records_of_kind(None, MAX_INPUT_NODES + 1)?;
            if names.len() + records.len() > MAX_INPUT_NODES {
                return Err(StoreError::QueryRefused(
                    "coarse Sophia metadata limit".into(),
                ));
            }
            let mut references = Vec::new();
            for id in &records {
                let targets = tx.refs_of(id)?;
                if references.len() + targets.len() > MAX_INPUT_EDGES {
                    return Err(StoreError::QueryRefused("coarse Sophia edge limit".into()));
                }
                references.extend(targets.into_iter().map(|target| (*id, target)));
            }
            Ok((names, records, references))
        })
        .map_err(store_error)?;
    // Definitions get short content-id labels in a coarse navigation snapshot;
    // opening a definition still uses the complete normal Source API.
    for id in records
        .into_iter()
        .chain(names.iter().map(|n| n.target.digest()))
    {
        let key = Key::Content(id);
        nodes.entry(key.clone()).or_insert_with(|| NodeView {
            key,
            kind: "Definition".into(),
            label: id.short(12),
            props: Vec::new(),
        });
        if nodes.len() > MAX_INPUT_NODES {
            return Err(limit());
        }
    }
    for name in names {
        edges.push(EdgeView {
            from: Key::name(&name.path),
            to: Key::Content(name.target.digest()),
            kind: "Names".into(),
            props: Vec::new(),
        });
    }
    for (from, to) in references {
        let (from, to) = (Key::Content(from), Key::Content(to));
        if nodes.contains_key(&from) && nodes.contains_key(&to) {
            edges.push(EdgeView {
                from,
                to,
                kind: "References".into(),
                props: Vec::new(),
            });
        }
    }
    if edges.len() > MAX_INPUT_EDGES {
        return Err(limit());
    }
    let end = interface
        .store()
        .read(|tx| tx.last_seq())
        .map_err(store_error)?;
    if start != end {
        return Err(SourceError::Invalid(
            "coarse Sophia snapshot changed".into(),
        ));
    }
    Ok(Subgraph {
        nodes: nodes.into_values().collect(),
        edges,
        ..Default::default()
    })
}

/// Bound reads and keep paths from stored metadata inside the opened folder.
fn read_current(root: &Path, relative: &str) -> Result<String, SourceError> {
    use std::io::Read;
    if relative.is_empty()
        || relative.split('/').any(|s| s.starts_with('.'))
        || !matches!(
            Path::new(relative).extension().and_then(|s| s.to_str()),
            Some("rs" | "py" | "jl")
        )
        || relative.contains('\\')
        || !Path::new(relative)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(SourceError::Invalid("invalid derived source path".into()));
    }
    let root = root
        .canonicalize()
        .map_err(|e| SourceError::Io(e.to_string()))?;
    let mut candidate = root.clone();
    for part in Path::new(relative).components() {
        candidate.push(part);
        if candidate
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(SourceError::Invalid(
                "derived source path is a symlink".into(),
            ));
        }
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| SourceError::NotFound)?;
    if !path.starts_with(&root) {
        return Err(SourceError::Invalid("derived source leaves folder".into()));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| SourceError::NotFound)?
        .take(512 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SourceError::Io(e.to_string()))?;
    if bytes.len() > 512 * 1024 {
        return Err(SourceError::Invalid("derived source text limit".into()));
    }
    String::from_utf8(bytes).map_err(|_| SourceError::Invalid("derived source is not UTF-8".into()))
}

/// File-owned import annotations resolve only through a matching current
/// manifest. An old fact cannot point at a new definition occupying its span.
fn import_edges(
    interface: &Interface<Sqlite>,
    graph: &mut Subgraph,
    limit: usize,
) -> Result<(), SourceError> {
    let subjects: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|n| match n.key {
            Key::Content(d) => Some(d),
            _ => None,
        })
        .collect();
    let mut examined = 0;
    for subject in subjects {
        for fact in interface
            .store()
            .read(|tx| tx.facts_of(&subject))
            .map_err(store_error)?
        {
            if fact.kind != "import_edge" || fact.by != "moonkale-index" {
                continue;
            }
            examined += 1;
            if examined > NEIGHBOUR_LIMIT || graph.edges.len() >= NEIGHBOUR_LIMIT {
                return Ok(());
            }
            if fact.value.len() > 64 * 1024 {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&fact.value) else {
                continue;
            };
            let (Some(path), Some(source), Some(start), Some(end), Some(kind)) = (
                value["path"].as_str(),
                value["source"].as_str(),
                value["start"].as_u64(),
                value["end"].as_u64(),
                value["kind"].as_str(),
            ) else {
                continue;
            };
            if !matches!(kind, "Calls" | "References") {
                continue;
            }
            let Some(manifest) = interface.store().file_manifest(path).map_err(store_error)? else {
                continue;
            };
            if manifest.source.is_none_or(|d| d.to_hex() != source) {
                continue;
            }
            let Some((target, _, _)) = manifest
                .spans
                .iter()
                .find(|(_, a, b)| u64::from(*a) == start && u64::from(*b) == end)
            else {
                continue;
            };
            let key = Key::Content(*target);
            if !graph.nodes.iter().any(|n| n.key == key) {
                if graph.nodes.len() >= limit {
                    continue;
                }
                let Some(node) = interface.node(&key).map_err(store_error)? else {
                    continue;
                };
                graph.nodes.push(node);
            }
            let Ok(props) =
                serde_json::from_value::<moonkale_core::graph::Properties>(value["props"].clone())
            else {
                continue;
            };
            let mut props: Vec<_> = props
                .into_iter()
                .filter_map(|(k, v)| match v {
                    Value::Text(s) => Some((k, Prop::Text(s))),
                    Value::Int(i) => Some((k, Prop::Int(i))),
                    Value::Bool(b) => Some((k, Prop::Bool(b))),
                    _ => None,
                })
                .collect();
            if let Some(origin) = value["origin"].as_str() {
                props.push((
                    "origins".into(),
                    Prop::Text(
                        serde_json::to_string(&vec![origin])
                            .map_err(|e| SourceError::Invalid(e.to_string()))?,
                    ),
                ));
            }
            let edge = EdgeView {
                from: Key::Content(subject),
                to: key,
                kind: kind.into(),
                props,
            };
            if let Some(existing) = graph
                .edges
                .iter_mut()
                .find(|e| e.from == edge.from && e.to == edge.to && e.kind == edge.kind)
            {
                // Content identities can merge callers from different files.
                // Core edge identity remains (source, from, to, kind).
                for (key, value) in edge.props {
                    if let Some((_, old)) = existing.props.iter_mut().find(|(k, _)| k == &key) {
                        match (key.as_str(), old, value) {
                            ("count", Prop::Int(a), Prop::Int(b)) => *a += b,
                            ("ranges", Prop::Text(a), Prop::Text(b)) => {
                                a.push(',');
                                a.push_str(&b);
                            }
                            ("imports" | "origins", Prop::Text(a), Prop::Text(b)) => {
                                if let (Ok(mut all), Ok(more)) = (
                                    serde_json::from_str::<BTreeSet<String>>(a),
                                    serde_json::from_str::<BTreeSet<String>>(&b),
                                ) {
                                    all.extend(more);
                                    *a = serde_json::to_string(&all)
                                        .map_err(|e| SourceError::Invalid(e.to_string()))?;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        existing.props.push((key, value));
                    }
                }
            } else {
                graph.edges.push(edge);
            }
        }
    }
    Ok(())
}

/// The folder writer stores generic targets as paths: nominal ids are allocated
/// inside replace_files. Resolve those annotations at read time, never in ids.
fn generic_calls(
    interface: &Interface<Sqlite>,
    graph: &mut Subgraph,
    limit: usize,
) -> Result<(), SourceError> {
    if !interface.store().is_derived().map_err(store_error)? {
        return Ok(());
    }
    let subjects: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|n| match n.key {
            Key::Content(d) => Some(d),
            _ => None,
        })
        .collect();
    for subject in subjects {
        let facts = interface
            .store()
            .read(|tx| tx.facts_of(&subject))
            .map_err(store_error)?;
        for fact in facts
            .into_iter()
            .filter(|f| f.kind == "uses_generic" && f.by == "moonkale-index")
        {
            let Ok(path) = std::str::from_utf8(&fact.value) else {
                continue;
            };
            let Some(name) = interface.store().resolve(path).map_err(store_error)? else {
                continue;
            };
            let key = Key::Content(name.target.digest());
            if !graph.nodes.iter().any(|n| n.key == key) {
                if graph.nodes.len() >= limit {
                    continue;
                }
                if let Some(node) = interface.node(&key).map_err(store_error)? {
                    graph.nodes.push(node);
                }
            }
            if graph.edges.len() >= NEIGHBOUR_LIMIT {
                return Ok(());
            }
            if !graph
                .edges
                .iter()
                .any(|e| e.from == Key::Content(subject) && e.to == key && e.kind == "Calls")
            {
                graph.edges.push(EdgeView {
                    from: Key::Content(subject),
                    to: key,
                    kind: "Calls".into(),
                    props: vec![],
                });
            }
        }
    }
    import_edges(interface, graph, limit)?;
    Ok(())
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
    #[test]
    fn edit_bases_bound_retention_and_distinguish_prints() {
        let source = SophiaSource::sample().unwrap();
        let node = NodeId::derive(&source.id(), "name:sample.user");
        let first = source.remember_text(node, 1, "old print");
        assert_ne!(first, source.remember_text(node, 1, "new print"));
        assert_ne!(first, source.remember_text(node, 2, "old print"));
        for i in 0..MAX_EDIT_BASES + 1 {
            source.remember_text(node, i as u64, &format!("print {i}"));
        }
        let bases = source.edit_bases.lock().unwrap();
        assert_eq!(bases.len(), MAX_EDIT_BASES);
        assert!(!bases.iter().any(|b| b.version == first));
        drop(bases);
        source.remember_text(node, 1, &"x".repeat(MAX_EDIT_BASE_TEXT + 1));
        assert_eq!(source.edit_bases.lock().unwrap().len(), MAX_EDIT_BASES);
        let text = "x".repeat(MAX_EDIT_BASE_TEXT);
        for token in 0..20 {
            source.remember_text(node, token, &text);
        }
        assert!(
            source
                .edit_bases
                .lock()
                .unwrap()
                .iter()
                .map(|b| b.text.len())
                .sum::<usize>()
                <= MAX_EDIT_BASE_BYTES
        );
    }

    #[tokio::test]
    async fn unchanged_prints_remain_savable_after_base_eviction() {
        let source = SophiaSource::sample().unwrap();
        let node = source.remember(&Key::name("sample.double"));
        let (text, version) = source.fetch_text(node).await.unwrap();
        source.edit_bases.lock().unwrap().clear();
        let applied = source
            .apply(Transaction::write_text(
                node,
                version,
                moonkale_core::TextPatch::whole(&text, text.chars().count()),
            ))
            .await
            .unwrap();
        assert!(applied.first_error().is_none());
        assert_eq!(
            applied.version_of(node),
            Some(source.fetch_text(node).await.unwrap().1)
        );
    }
}
