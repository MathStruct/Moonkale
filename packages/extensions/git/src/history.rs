//! History as a graph: commits and the files they touched, as an in-memory
//! [`Source`] the Graph panel can pick (like a trace, Milestone 4).

use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, EdgeKind, Node, NodeId, NodeKind, Query, QueryResult,
    Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Transaction, Version,
};
use moonkale_ext_api::git::Commit;

pub struct GitHistorySource {
    id: SourceId,
    root: NodeId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl GitHistorySource {
    /// `unique` keeps two snapshots of the same repository apart.
    pub fn new(folder: &str, commits: &[Commit], unique: u64) -> Self {
        let id = SourceId::new(format!("git:{folder}#{unique}"));
        let root = NodeId::derive(&id, "");
        let mut nodes = vec![Node {
            id: root,
            source: id.clone(),
            kind: NodeKind::Directory,
            label: "history".into(),
            native_key: String::new(),
            content: None,
            version: Version::default(),
        }];
        let mut edges = Vec::new();
        let commit_id = |hash: &str| NodeId::derive(&id, &format!("commit:{hash}"));
        let mut files: Vec<String> = Vec::new();
        for c in commits {
            let cid = commit_id(&c.hash);
            nodes.push(Node {
                id: cid,
                source: id.clone(),
                kind: NodeKind::Custom("commit".into()),
                label: format!(
                    "{} {}",
                    c.short,
                    c.subject.chars().take(48).collect::<String>()
                ),
                native_key: format!("commit:{}", c.hash),
                content: None,
                version: Version::default(),
            });
            edges.push(Edge::contains(&id, root, cid));
            for p in &c.parents {
                if commits.iter().any(|x| &x.hash == p) {
                    edges.push(Edge {
                        source: id.clone(),
                        from: cid,
                        to: commit_id(p),
                        kind: EdgeKind::Custom("parent".into()),
                    });
                }
            }
            for f in &c.files {
                if !files.contains(f) {
                    files.push(f.clone());
                }
                edges.push(Edge {
                    source: id.clone(),
                    from: cid,
                    to: NodeId::derive(&id, f),
                    kind: EdgeKind::Custom("touches".into()),
                });
            }
        }
        for f in files {
            nodes.push(Node {
                id: NodeId::derive(&id, &f),
                source: id.clone(),
                kind: NodeKind::File,
                label: f.rsplit('/').next().unwrap_or(&f).to_string(),
                native_key: f,
                content: None,
                version: Version::default(),
            });
        }
        Self {
            id,
            root,
            nodes,
            edges,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for GitHistorySource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            display_name: format!(
                "git history ({} commits)",
                self.nodes
                    .iter()
                    .filter(|n| matches!(&n.kind, NodeKind::Custom(c) if c == "commit"))
                    .count()
            ),
            family: SourceFamily::Custom("git".into()),
            capabilities: Capabilities {
                read: true,
                write: false,
                watch: false,
                text_query: None,
            },
            root: self.root,
        }
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        let node = |id: NodeId| self.nodes.iter().find(|n| n.id == id).cloned();
        match query {
            Query::Node(id) => node(id)
                .map(QueryResult::single)
                .ok_or(SourceError::NotFound),
            Query::Children(id) => {
                let edges: Vec<Edge> = self
                    .edges
                    .iter()
                    .filter(|e| e.from == id && e.kind == EdgeKind::Contains)
                    .cloned()
                    .collect();
                let nodes = edges.iter().filter_map(|e| node(e.to)).collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    ..Default::default()
                })
            }
            Query::Neighbours { node: id, .. } => {
                let edges: Vec<Edge> = self
                    .edges
                    .iter()
                    .filter(|e| e.from == id || e.to == id)
                    .cloned()
                    .collect();
                let nodes = edges
                    .iter()
                    .filter_map(|e| node(if e.from == id { e.to } else { e.from }))
                    .collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    ..Default::default()
                })
            }
            Query::All { limit, .. } => {
                let mut nodes: Vec<Node> = self
                    .nodes
                    .iter()
                    .filter(|n| n.id != self.root)
                    .cloned()
                    .collect();
                let truncated = nodes.len() > limit;
                nodes.truncate(limit);
                let edges = self
                    .edges
                    .iter()
                    .filter(|e| e.kind != EdgeKind::Contains)
                    .cloned()
                    .collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    table: None,
                    truncated,
                })
            }
            Query::Text { .. } => Err(SourceError::Unsupported(
                "git history has no query language".into(),
            )),
        }
    }

    async fn fetch_text(&self, _node: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::Unsupported(
            "history nodes point at files in the folder; open those".into(),
        ))
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("history is read-only".into()))
    }
}
