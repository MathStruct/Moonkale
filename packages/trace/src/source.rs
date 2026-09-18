//! A parsed trace as an in-memory `Source`.
//!
//! Nodes: root (`Custom("trace")`, the title) → files (`File`, key = path
//! as printed) → frames (`Symbol`, key = `path:line[:col]`, label = function
//! or `path:line`). Edges: `Contains` root→file and file→frame; `Calls`
//! frame→next frame in listing order.

use crate::parse::Trace;
use moonkale_core::{
    async_trait, Applied, Capabilities, Edge, EdgeKind, Node, NodeId, NodeKind, Query, QueryResult,
    Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Transaction, Version,
};

pub struct TraceSource {
    id: SourceId,
    root: NodeId,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    title: String,
}

impl TraceSource {
    /// `unique` distinguishes several traces from the same text (a counter).
    pub fn new(trace: &Trace, unique: u64) -> Self {
        let id = SourceId::new(format!("trace:{unique}"));
        let make = |key: &str, kind: NodeKind, label: String| Node {
            id: NodeId::derive(&id, key),
            source: id.clone(),
            kind,
            label,
            native_key: key.to_string(),
            content: None,
            version: Version::default(),
        };
        let root = make("", NodeKind::Custom("trace".into()), trace.title.clone());
        let mut nodes = vec![root.clone()];
        let mut edges = Vec::new();
        let mut prev: Option<NodeId> = None;
        for f in &trace.frames {
            let file_key = f.file.clone();
            let file_id = NodeId::derive(&id, &file_key);
            if !nodes.iter().any(|n| n.id == file_id) {
                let name = f.file.rsplit('/').next().unwrap_or(&f.file).to_string();
                nodes.push(make(&file_key, NodeKind::File, name));
                edges.push(Edge::contains(&id, root.id, file_id));
            }
            let frame_key = match f.col {
                Some(c) => format!("{}:{}:{}", f.file, f.line, c),
                None => format!("{}:{}", f.file, f.line),
            };
            let frame_id = NodeId::derive(&id, &frame_key);
            if !nodes.iter().any(|n| n.id == frame_id) {
                let label = f.function.clone().unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        f.file.rsplit('/').next().unwrap_or(&f.file),
                        f.line
                    )
                });
                nodes.push(make(&frame_key, NodeKind::Symbol, label));
                edges.push(Edge::contains(&id, file_id, frame_id));
            }
            if let Some(p) = prev {
                if p != frame_id {
                    edges.push(Edge {
                        source: id.clone(),
                        from: p,
                        to: frame_id,
                        kind: EdgeKind::Calls,
                    });
                }
            }
            prev = Some(frame_id);
        }
        Self {
            root: root.id,
            id,
            nodes,
            edges,
            title: trace.title.clone(),
        }
    }

    pub fn root_id(&self) -> NodeId {
        self.root
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for TraceSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            display_name: format!("trace: {}", self.title.chars().take(40).collect::<String>()),
            family: SourceFamily::Custom("trace".into()),
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
                let mut nodes = self.nodes.clone();
                let truncated = nodes.len() > limit;
                nodes.truncate(limit);
                Ok(QueryResult {
                    nodes,
                    edges: self.edges.clone(),
                    table: None,
                    truncated,
                })
            }
            Query::Text { .. } => Err(SourceError::Unsupported(
                "a trace has no query language".into(),
            )),
        }
    }

    async fn fetch_text(&self, _node: NodeId) -> Result<(String, Version), SourceError> {
        Err(SourceError::Unsupported(
            "trace nodes point at files in the folder; open those".into(),
        ))
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported("traces are read-only".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn trace_becomes_files_frames_and_a_chain() {
        let t = crate::parse("thread 'main' panicked at src/main.rs:7:5:\nboom\nstack backtrace:\n   0: a::inner\n             at ./src/lib.rs:3:5\n   1: a::main\n             at ./src/main.rs:7:5\n");
        let s = TraceSource::new(&t[0], 1);
        let all = s
            .query(Query::All {
                limit: 100,
                kinds: None,
            })
            .await
            .unwrap();
        // root + 3 files (src/main.rs, ./src/lib.rs, ./src/main.rs) + 3 frames
        assert_eq!(
            all.nodes.len(),
            7,
            "{:?}",
            all.nodes.iter().map(|n| &n.native_key).collect::<Vec<_>>()
        );
        assert_eq!(
            all.edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .count(),
            2
        );
        let frames: Vec<&Node> = all
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Symbol)
            .collect();
        assert_eq!(frames[1].native_key, "./src/lib.rs:3:5");
        assert_eq!(frames[1].label, "a::inner");
        let kids = s.query(Query::Children(s.root_id())).await.unwrap();
        assert_eq!(kids.nodes.len(), 3);
    }
}
