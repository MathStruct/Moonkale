use moonkale_core::{
    graph::{code, ContentRef, Edge, EdgeKind, Node, NodeKind, Value, Version},
    source::Direction,
    Applied, Capabilities, NodeId, Query, QueryResult, Source, SourceDescriptor, SourceError,
    SourceFamily, SourceId, SourceTruth, Transaction,
};
use serde_json::Value as Json;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const MAX_NODES: usize = 4096;
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(60);
fn invalid() -> SourceError {
    SourceError::Invalid("unsupported or oversized UCM 1.5 API response".into())
}
fn io_error() -> SourceError {
    SourceError::Io("local UCM request failed (requires UCM 1.5)".into())
}
fn field<'a>(j: &'a Json, key: &str) -> Result<&'a str, SourceError> {
    j[key].as_str().ok_or_else(invalid)
}
fn array<'a>(j: &'a Json, key: &str) -> Result<&'a Vec<Json>, SourceError> {
    j[key].as_array().ok_or_else(invalid)
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Api {
    base: reqwest::Url,
    client: reqwest::blocking::Client,
    start: Instant,
    bytes: u64,
    requests: usize,
}
impl Api {
    fn get(&mut self, parts: &[&str], query: &[(&str, &str)]) -> Result<Json, SourceError> {
        if self.start.elapsed() >= DEADLINE || self.requests >= 16384 {
            return Err(invalid());
        }
        self.requests += 1;
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| invalid())?
            .pop_if_empty()
            .extend(parts);
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        let response = self
            .client
            .get(url)
            .timeout(
                DEADLINE
                    .saturating_sub(self.start.elapsed())
                    .min(Duration::from_secs(5)),
            )
            .send()
            .map_err(|_| io_error())?;
        if !response.status().is_success() {
            return Err(io_error());
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BYTES.saturating_sub(self.bytes) + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| io_error())?;
        self.bytes += bytes.len() as u64;
        if self.bytes > MAX_BYTES {
            return Err(invalid());
        }
        serde_json::from_slice(&bytes).map_err(|_| invalid())
    }
}

/// An immutable, finite snapshot; reopening observes external UCM changes.
pub struct UnisonSource {
    id: SourceId,
    root: NodeId,
    nodes: BTreeMap<NodeId, Node>,
    edges: Vec<Edge>,
    edge_keys: BTreeSet<(NodeId, NodeId, String)>,
    texts: BTreeMap<NodeId, String>,
}
impl UnisonSource {
    /// Executable discovery is operator-owned: MOONKALE_UCM, then PATH. Never from source data.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, SourceError> {
        let path = path.into();
        tokio::task::spawn_blocking(move || Self::load(&path))
            .await
            .map_err(|_| io_error())?
    }
    fn node(
        &mut self,
        key: &str,
        label: &str,
        kind: NodeKind,
        id: Option<NodeId>,
    ) -> Result<NodeId, SourceError> {
        let id = id.unwrap_or_else(|| NodeId::derive(&self.id, key));
        if !self.nodes.contains_key(&id) {
            if self.nodes.len() >= MAX_NODES {
                return Err(invalid());
            }
            self.nodes.insert(
                id,
                Node {
                    id,
                    source: self.id.clone(),
                    kind,
                    label: label.into(),
                    native_key: key.into(),
                    content: None,
                    version: Version(0),
                    props: Default::default(),
                },
            );
        }
        Ok(id)
    }
    fn edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> Result<(), SourceError> {
        if self.edge_keys.insert((from, to, format!("{kind:?}"))) {
            if self.edges.len() >= MAX_NODES * 16 {
                return Err(invalid());
            }
            self.edges.push(Edge {
                source: self.id.clone(),
                from,
                to,
                kind,
                props: Default::default(),
            });
        }
        Ok(())
    }
    fn definition(&mut self, hash: &str, kind: &str) -> Result<NodeId, SourceError> {
        let canonical = canonical_hash(hash)?;
        let id = NodeId::from_content(canonical.as_bytes());
        self.node(&canonical, &canonical, NodeKind::definition(), Some(id))?;
        let n = self.nodes.get_mut(&id).ok_or_else(invalid)?;
        for (k, v) in [
            ("content_id", canonical.as_str()),
            ("lang", "unison"),
            ("def_kind", kind),
            ("frontend", "ucm-http-1.5"),
        ] {
            n.props.insert(k.into(), Value::Text(v.into()));
        }
        if !canonical.starts_with("##") {
            let (component, index) = canonical.split_once('.').unwrap_or((&canonical, "0"));
            let component = component.split('#').nth(1).ok_or_else(invalid)?;
            n.props.insert(
                "content_component".into(),
                Value::Text(format!("#{component}")),
            );
            n.props.insert(
                "content_component_index".into(),
                Value::Int(
                    index
                        .split('#')
                        .next()
                        .unwrap_or("0")
                        .parse()
                        .map_err(|_| invalid())?,
                ),
            );
        }
        Ok(id)
    }
    fn load(path: &Path) -> Result<Self, SourceError> {
        let path = path.canonicalize().map_err(|_| io_error())?;
        if !path.join(".unison/v2/unison.sqlite3").is_file() {
            return Err(SourceError::Invalid(
                "expected a Unison codebase root containing .unison/v2/unison.sqlite3".into(),
            ));
        }
        // UCM owns its SQLite files. Refuse pre-existing store symlinks rather than
        // allowing an opened source to redirect this process outside its root.
        for relative in [
            ".unison",
            ".unison/v2",
            ".unison/v2/unison.sqlite3",
            ".unison/v2/unison.sqlite3-wal",
            ".unison/v2/unison.sqlite3-shm",
            ".unison/v2/unison.lockfile",
        ] {
            match std::fs::symlink_metadata(path.join(relative)) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(SourceError::Invalid(
                        "Unison store symlinks are unsupported".into(),
                    ))
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io_error()),
                _ => {}
            }
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| io_error())?;
        let port = listener.local_addr().map_err(|_| io_error())?.port();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| io_error())?;
        drop(listener);
        let mut process = Process(
            Command::new(std::env::var_os("MOONKALE_UCM").unwrap_or_else(|| "ucm".into()))
                .arg("-c")
                .arg(&path)
                .args([
                    "headless",
                    "--host",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--token",
                    &token,
                    "--no-file-watch",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| {
                    SourceError::Io(
                        "UCM missing or could not start; set MOONKALE_UCM or PATH".into(),
                    )
                })?,
        );
        let mut api = Api {
            base: reqwest::Url::parse(&format!("http://127.0.0.1:{port}/{token}/api/"))
                .map_err(|_| invalid())?,
            client,
            start: Instant::now(),
            bytes: 0,
            requests: 0,
        };
        let projects = loop {
            match api.get(&["projects"], &[]) {
                Ok(j) => break j,
                Err(_)
                    if api.start.elapsed() < Duration::from_secs(10)
                        && process.0.try_wait().map_err(|_| io_error())?.is_none() =>
                {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(e) => return Err(e),
            }
        };
        let id = SourceId::new(format!("unison:{}", path.display()));
        let root = NodeId::derive(&id, "ns:");
        let mut out = Self {
            id,
            root,
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            edge_keys: BTreeSet::new(),
            texts: BTreeMap::new(),
        };
        out.node("ns:", "Unison", NodeKind::Directory, None)?;
        for p in projects.as_array().ok_or_else(invalid)? {
            let project = field(p, "projectName")?;
            let branches = api.get(&["projects", project, "branches"], &[])?;
            for b in branches.as_array().ok_or_else(invalid)? {
                let branch = field(b, "branchName")?;
                out.branch(&mut api, project, branch)?;
            }
        }
        Ok(out)
    }
    fn resolve_hash(
        &self,
        api: &mut Api,
        endpoint: &[&str],
        hash: &str,
    ) -> Result<String, SourceError> {
        if let Ok(full) = canonical_hash(hash) {
            return Ok(full);
        }
        // Dependency summaries may abbreviate hashes even though namespace listings do not.
        // Ask UCM to resolve the hash; never turn an abbreviation into an identity.
        let result = api.get(endpoint, &[("names", hash)])?;
        let keys: Vec<_> = ["termDefinitions", "typeDefinitions"]
            .iter()
            .flat_map(|key| result[*key].as_object().into_iter().flat_map(|m| m.keys()))
            .collect();
        if keys.len() != 1 {
            return Err(invalid());
        }
        canonical_hash(keys[0])
    }
    fn branch(&mut self, api: &mut Api, project: &str, branch: &str) -> Result<(), SourceError> {
        let prefix = format!("{project}/{branch}");
        let branch_root =
            self.node(&format!("ns:{prefix}:"), &prefix, NodeKind::Directory, None)?;
        self.edge(self.root, branch_root, EdgeKind::Contains)?;
        let parts = |endpoint| ["projects", project, "branches", branch, endpoint];
        let root_listing = api.get(&parts("list"), &[])?;
        let root_hash = field(&root_listing, "namespaceListingHash")?.to_owned();
        let mut queue = VecDeque::from([(String::new(), branch_root, 0)]);
        let mut definitions = BTreeMap::new();
        while let Some((namespace, parent, depth)) = queue.pop_front() {
            if api.start.elapsed() >= DEADLINE {
                return Err(invalid());
            }
            if depth > 64 {
                return Err(invalid());
            }
            let params = [("namespace", namespace.as_str())];
            let listing = api.get(
                &parts("list"),
                if namespace.is_empty() { &[] } else { &params },
            )?;
            for child in array(&listing, "namespaceListingChildren")? {
                let c = &child["contents"];
                let tag = field(child, "tag")?;
                let name_key = if tag == "Subnamespace" {
                    "namespaceName"
                } else if tag == "TypeObject" {
                    "typeName"
                } else if tag == "TermObject" {
                    "termName"
                } else {
                    return Err(SourceError::Unsupported(
                        "Unison patches are not supported".into(),
                    ));
                };
                let local = field(c, name_key)?.split('#').next().ok_or_else(invalid)?;
                let name = if namespace.is_empty() {
                    local.to_owned()
                } else {
                    format!("{namespace}.{local}")
                };
                if tag == "Subnamespace" {
                    let ns = self.node(
                        &format!("ns:{prefix}:{name}"),
                        local,
                        NodeKind::Directory,
                        None,
                    )?;
                    self.edge(parent, ns, EdgeKind::Contains)?;
                    queue.push_back((name, ns, depth + 1));
                } else {
                    let hash = field(
                        c,
                        if tag == "TypeObject" {
                            "typeHash"
                        } else {
                            "termHash"
                        },
                    )?;
                    let kind = if tag == "TypeObject" {
                        if c["typeTag"] == "Ability" {
                            "interface"
                        } else {
                            "type"
                        }
                    } else {
                        term_kind(c, "termTag", "termType")?
                    };
                    let hash = self.resolve_hash(api, &parts("getDefinition"), hash)?;
                    let def = self.definition(&hash, kind)?;
                    let name_id = self.node(
                        &format!(
                            "name:{prefix}:{name}:{}",
                            if tag == "TypeObject" { "type" } else { "term" }
                        ),
                        &name,
                        NodeKind::Custom(code::NAME.into()),
                        None,
                    )?;
                    self.edge(parent, name_id, EdgeKind::Contains)?;
                    self.edge(name_id, def, EdgeKind::Custom(code::NAMES.into()))?;
                    definitions.entry(def).or_insert(name);
                }
            }
        }
        for (def, _name) in definitions {
            let hash = self.nodes[&def].native_key.clone();
            let hash = if hash.starts_with("##") {
                hash
            } else {
                format!("#{}", hash[1..].split('#').next().ok_or_else(invalid)?)
            };
            let display = api.get(
                &parts("getDefinition"),
                &[("names", &hash), ("suffixifyBindings", "false")],
            )?;
            if !array(&display, "missingDefinitions")?.is_empty() {
                return Err(SourceError::Invalid(
                    "Unison definition unavailable while loading".into(),
                ));
            }
            for (key, is_type) in [("termDefinitions", false), ("typeDefinitions", true)] {
                for (hash, d) in display[key].as_object().ok_or_else(invalid)? {
                    let kind = if is_type {
                        if d["defnTypeTag"] == "Ability" {
                            "interface"
                        } else {
                            "type"
                        }
                    } else {
                        term_kind(d, "defnTermTag", "signature")?
                    };
                    let id = self.definition(hash, kind)?;
                    // A constructor request returns its parent type's declaration.
                    let object = &d[if is_type {
                        "typeDefinition"
                    } else {
                        "termDefinition"
                    }];
                    let text = syntax_text(&object["contents"])?;
                    if !text.is_empty() {
                        self.texts.insert(id, text.clone());
                        if def != id
                            && self.nodes[&def].props.get("def_kind")
                                == Some(&Value::Text("variant".into()))
                        {
                            self.texts.insert(def, text);
                            self.edge(def, id, EdgeKind::Custom(code::TYPE_OF.into()))?;
                        }
                    }
                    if !is_type {
                        for hash in exact_type_ref(&d["signature"])? {
                            let hash = self.resolve_hash(api, &parts("getDefinition"), &hash)?;
                            let target = self.definition(&hash, "type")?;
                            self.edge(id, target, EdgeKind::Custom(code::TYPE_OF.into()))?;
                        }
                    }
                }
            }
            let deps = api.get(&parts("getDefinitionDependencies"), &[("name", &hash)])?;
            for dep in array(&deps, "results")? {
                let d = &dep["definition"];
                let kind = if dep["kind"] == "type" {
                    if d["tag"] == "Ability" {
                        "interface"
                    } else {
                        "type"
                    }
                } else {
                    match field(d, "tag")? {
                        "DataConstructor" | "AbilityConstructor" => "variant",
                        "Test" => "test",
                        _ if syntax_text(&d["summary"]["contents"])?.contains("->") => "function",
                        _ => "constant",
                    }
                };
                let hash = self.resolve_hash(api, &parts("getDefinition"), field(d, "hash")?)?;
                let target = self.definition(&hash, kind)?;
                self.edge(def, target, EdgeKind::References)?;
            }
        }
        let after = api.get(&parts("list"), &[])?;
        if field(&after, "namespaceListingHash")? != root_hash {
            return Err(SourceError::Invalid(
                "Unison branch changed while loading; reopen".into(),
            ));
        }
        for (id, text) in &self.texts {
            if let Some(node) = self.nodes.get_mut(id) {
                node.content = Some(ContentRef::Text {
                    len: text.len() as u64,
                    lang: Some("unison".into()),
                });
            }
        }
        Ok(())
    }
}
fn term_kind(j: &Json, tag: &str, signature: &str) -> Result<&'static str, SourceError> {
    Ok(match field(j, tag)? {
        "DataConstructor" | "AbilityConstructor" => "variant",
        "Test" => "test",
        _ => {
            if syntax_text(&j[signature])?.contains("->") {
                "function"
            } else {
                "constant"
            }
        }
    })
}
fn syntax_text(j: &Json) -> Result<String, SourceError> {
    if j.is_null() {
        return Ok(String::new());
    }
    j.as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|s| field(s, "segment"))
        .collect::<Result<Vec<_>, _>>()
        .map(|s| s.concat())
}
fn exact_type_ref(j: &Json) -> Result<BTreeSet<String>, SourceError> {
    let mut refs = BTreeSet::new();
    let segments: Vec<_> = j
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .filter(|s| s["segment"].as_str().is_some_and(|s| !s.trim().is_empty()))
        .collect();
    if segments.len() != 1 {
        return Ok(refs);
    }
    for s in segments {
        let a = &s["annotation"];
        if a["tag"] == "TypeReference" {
            refs.insert(field(a, "contents")?.into());
        }
    }
    Ok(refs)
}
// Full base32hex digest plus component and constructor selectors. UCM prints index zero
// without `.0`; listing constructors use `#0`, syntax uses `#d0` / `#a0`.
fn canonical_hash(hash: &str) -> Result<String, SourceError> {
    if hash.len() > 512 {
        return Err(invalid());
    }
    if hash.starts_with("##") && hash.len() > 2 && !hash.chars().any(char::is_control) {
        return Ok(hash.into());
    }
    let tail = hash.strip_prefix('#').ok_or_else(invalid)?;
    let (reference, constructor) = tail
        .split_once('#')
        .map_or((tail, None), |(a, b)| (a, Some(b)));
    let (digest, index) = reference.split_once('.').unwrap_or((reference, "0"));
    if digest.len() != 103
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'v').contains(&b))
        || index.parse::<u32>().is_err()
    {
        return Err(invalid());
    }
    let mut out = format!("#{digest}");
    let index: u32 = index.parse().map_err(|_| invalid())?;
    if index != 0 {
        out.push_str(&format!(".{index}"));
    }
    if let Some(c) = constructor {
        let c = c.trim_start_matches(['d', 'a']);
        let c: u32 = c.parse().map_err(|_| invalid())?;
        out.push_str(&format!("#{c}"));
    }
    Ok(out)
}

#[moonkale_core::async_trait]
impl Source for UnisonSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            truth: SourceTruth::Store,
            id: self.id(),
            display_name: "Unison".into(),
            family: SourceFamily::Graph,
            capabilities: Capabilities {
                read: true,
                ..Default::default()
            },
            root: self.root,
        }
    }
    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        let mut selected = BTreeSet::new();
        let mut truncated = false;
        match query {
            Query::Coarse { level, region } => {
                return moonkale_core::source::coarse::query(self, level, region).await;
            }
            Query::Node(id) => {
                if !self.nodes.contains_key(&id) {
                    return Err(SourceError::NotFound);
                }
                selected.insert(id);
            }
            Query::All { limit, kinds } => {
                let matching: Vec<_> = self
                    .nodes
                    .values()
                    .filter(|n| kinds.as_ref().is_none_or(|k| k.contains(&n.kind)))
                    .collect();
                truncated = matching.len() > limit;
                selected.extend(matching.into_iter().take(limit).map(|n| n.id));
            }
            Query::Children(id) => {
                selected.extend(
                    self.edges
                        .iter()
                        .filter(|e| e.from == id && e.kind == EdgeKind::Contains)
                        .map(|e| e.to),
                );
            }
            Query::Neighbours {
                node,
                depth,
                direction,
            } => {
                if !self.nodes.contains_key(&node) {
                    return Err(SourceError::NotFound);
                }
                selected.insert(node);
                for _ in 0..depth.min(64) {
                    let mut next = selected.clone();
                    for e in &self.edges {
                        if direction != Direction::In && selected.contains(&e.from) {
                            next.insert(e.to);
                        }
                        if direction != Direction::Out && selected.contains(&e.to) {
                            next.insert(e.from);
                        }
                    }
                    if next == selected {
                        break;
                    }
                    selected = next;
                }
                truncated = depth > 64;
            }
            Query::Text { .. } => {
                return Err(SourceError::Unsupported("Unison text queries".into()))
            }
        }
        Ok(QueryResult {
            nodes: selected
                .iter()
                .filter_map(|id| self.nodes.get(id).cloned())
                .collect(),
            edges: self
                .edges
                .iter()
                .filter(|e| selected.contains(&e.from) && selected.contains(&e.to))
                .cloned()
                .collect(),
            truncated,
            table: None,
        })
    }
    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        self.texts
            .get(&node)
            .cloned()
            .map(|t| (t, Version(0)))
            .ok_or(SourceError::NotFound)
    }
    async fn apply(&self, _: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("read-only Unison snapshot".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn full_references_preserve_component_and_constructor_identity() {
        let hash = format!("#{}", "a".repeat(103));
        assert_eq!(canonical_hash(&format!("{hash}.0")).unwrap(), hash);
        assert_ne!(canonical_hash(&format!("{hash}.1")).unwrap(), hash);
        assert_eq!(
            canonical_hash(&format!("{hash}#d2")).unwrap(),
            format!("{hash}#2")
        );
        assert_eq!(
            canonical_hash(&format!("{hash}#a2")).unwrap(),
            format!("{hash}#2")
        );
        assert!(canonical_hash("#abcdef1234").is_err());
        assert!(canonical_hash(&format!("{hash}.notanindex")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn store_symlinks_fail_before_starting_ucm() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("store");
        std::fs::create_dir_all(root.join(".unison/v2")).unwrap();
        let outside = temp.path().join("outside.sqlite3");
        std::fs::write(&outside, []).unwrap();
        std::os::unix::fs::symlink(outside, root.join(".unison/v2/unison.sqlite3")).unwrap();
        assert!(matches!(
            UnisonSource::load(&root),
            Err(SourceError::Invalid(_))
        ));
    }

    fn reply(body: &[u8], status: &str, bytes: u64) -> Result<Json, SourceError> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_vec();
        let status = status.to_owned();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nLocation: http://example.invalid/\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            let _ = stream.write_all(&body);
        });
        let mut api = Api {
            base: reqwest::Url::parse(&format!("http://{address}/")).unwrap(),
            client: reqwest::blocking::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            start: Instant::now(),
            bytes,
            requests: 0,
        };
        let result = api.get(&["projects"], &[]);
        worker.join().unwrap();
        result
    }

    #[test]
    fn api_rejects_redirects_oversized_and_deep_responses() {
        assert!(reply(b"{}", "302 Found", 0).is_err());
        assert!(matches!(
            reply(b"{}", "200 OK", MAX_BYTES - 1),
            Err(SourceError::Invalid(_))
        ));
        let deep = format!("{}0{}", "[".repeat(200), "]".repeat(200));
        assert!(matches!(
            reply(deep.as_bytes(), "200 OK", 0),
            Err(SourceError::Invalid(_))
        ));
        assert_eq!(reply(b"{}", "200 OK", 0).unwrap(), serde_json::json!({}));
    }
}
