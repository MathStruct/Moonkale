//! `IndexSource` — the derived graph as a `Source`.

use crate::extract::{symbols_rust, wikilinks, Derived};
use crate::graph::IndexGraph;
use crate::search::{Hit, SearchIndex};
use crate::walk::{self, Limits};
use moonkale_core::{
    async_trait, Applied, Capabilities, ContentRef, Edge, Node, NodeId, NodeKind, Query,
    QueryResult, Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Table, Transaction,
    Value, Version,
};
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexStats {
    pub files: usize,
    pub links: usize,
    pub symbols: usize,
    pub truncated: bool,
}

pub struct IndexSource {
    id: SourceId,
    folder: Arc<dyn Source>,
    root: NodeId,
    graph: RwLock<IndexGraph>,
    stats: RwLock<IndexStats>,
    limits: Limits,
    search: RwLock<SearchIndex>,
    /// Embedding provider for hybrid search (`None` = BM25 only).
    embedder: Option<Arc<dyn moonkale_llm::Provider>>,
}

/// Batch size for embedding requests.
const EMBED_BATCH: usize = 32;

impl IndexSource {
    /// Build the index of `folder` now. The folder's own root node becomes
    /// the index root, so the index's `Children(root)` is the folder's tree
    /// enriched with derived nodes.
    pub async fn build(folder: Arc<dyn Source>) -> Result<Self, SourceError> {
        Self::build_with(folder, None).await
    }

    /// Like [`build`](Self::build), with an embedding provider: chunks get
    /// vectors (call [`embed_pending`](Self::embed_pending) to fill them)
    /// and `search` fuses BM25 with cosine similarity.
    pub async fn build_with(
        folder: Arc<dyn Source>,
        embedder: Option<Arc<dyn moonkale_llm::Provider>>,
    ) -> Result<Self, SourceError> {
        let fd = folder.descriptor();
        let id = SourceId::new(format!("index:{}", fd.id));
        let this = Self {
            id,
            root: fd.root,
            folder,
            graph: RwLock::new(IndexGraph::default()),
            stats: RwLock::new(IndexStats::default()),
            limits: Limits::default(),
            search: RwLock::new(SearchIndex::default()),
            embedder: embedder.filter(|e| e.supports_embed()),
        };
        this.rebuild().await?;
        Ok(this)
    }

    /// Embed every chunk that has no vector yet, in batches. Safe to call
    /// repeatedly (after refreshes); a no-op without an embedder.
    pub async fn embed_pending(&self) -> Result<usize, String> {
        let Some(e) = &self.embedder else {
            return Ok(0);
        };
        let mut done = 0;
        loop {
            let batch = self.search.read().unwrap().pending_embeddings(EMBED_BATCH);
            if batch.is_empty() {
                break;
            }
            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
            let vectors = e.embed(texts).await?;
            let mut s = self.search.write().unwrap();
            for ((i, _), v) in batch.into_iter().zip(vectors) {
                s.set_embedding(i, v);
                done += 1;
            }
        }
        Ok(done)
    }

    /// `(chunks, embedded)` for the status line.
    pub fn search_stats(&self) -> (usize, usize) {
        let s = self.search.read().unwrap();
        (s.chunk_count(), s.embedded_count())
    }

    async fn run_search(&self, query: &str, limit: usize) -> Result<Vec<Hit>, SourceError> {
        let qv = match &self.embedder {
            Some(e) if self.search.read().unwrap().embedded_count() > 0 => e
                .embed(vec![query.to_string()])
                .await
                .ok()
                .and_then(|mut v| v.pop()),
            _ => None,
        };
        Ok(self
            .search
            .read()
            .unwrap()
            .search(query, qv.as_deref(), limit))
    }

    pub fn stats(&self) -> IndexStats {
        *self.stats.read().unwrap()
    }

    async fn rebuild(&self) -> Result<(), SourceError> {
        let root_node = self
            .folder
            .query(Query::Node(self.root))
            .await?
            .nodes
            .into_iter()
            .next();
        let (entries, truncated) = walk::walk(&*self.folder, self.root, &self.limits).await?;
        let mut graph = IndexGraph::default();
        if let Some(r) = root_node {
            graph.insert_node(r);
        }
        let mut contains: std::collections::HashMap<NodeId, Vec<Edge>> = Default::default();
        for (parent, node) in &entries {
            graph.insert_node(node.clone());
            contains.entry(*parent).or_default().push(Edge::contains(
                &node.source,
                *parent,
                node.id,
            ));
        }
        for (parent, edges) in contains {
            graph.set_derived(parent, Vec::new(), edges);
        }
        *self.graph.write().unwrap() = graph;
        *self.search.write().unwrap() = SearchIndex::default();

        let mut files = 0;
        for (_, node) in entries {
            if node.kind == NodeKind::File {
                files += 1;
                if walk::wants_text(&node, &self.limits) {
                    if let Ok((text, _)) = self.folder.fetch_text(node.id).await {
                        self.extract_into(&node, &text);
                    }
                }
            }
        }
        let g = self.graph.read().unwrap();
        *self.stats.write().unwrap() = IndexStats {
            files,
            links: g.count_edge_kind(&moonkale_core::EdgeKind::Links),
            symbols: g.count_kind(&NodeKind::Symbol),
            truncated,
        };
        Ok(())
    }

    fn extract_into(&self, file: &Node, text: &str) {
        self.search.write().unwrap().set_file(file, text);
        let derived = {
            let g = self.graph.read().unwrap();
            let mut d = Derived::default();
            match file.content.as_ref().and_then(|c| match c {
                ContentRef::Text { lang, .. } => lang.as_deref(),
                _ => None,
            }) {
                Some("markdown") => d.merge(wikilinks::extract(&g, &self.id, file, text)),
                Some("rust") => d.merge(symbols_rust::extract(&self.id, file, text)),
                _ => {}
            }
            d
        };
        self.graph
            .write()
            .unwrap()
            .set_derived(file.id, derived.nodes, derived.edges);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for IndexSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            display_name: {
                let s = self.stats();
                format!(
                    "index: {} files · {} links · {} symbols{}",
                    s.files,
                    s.links,
                    s.symbols,
                    if s.truncated { " (truncated)" } else { "" }
                )
            },
            family: SourceFamily::Index,
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
        let g = self.graph.read().unwrap();
        match query {
            Query::Node(id) => g
                .node(id)
                .cloned()
                .map(QueryResult::single)
                .ok_or(SourceError::NotFound),
            Query::Children(id) => {
                let ids = g.children(id);
                let nodes: Vec<Node> = ids.iter().filter_map(|i| g.node(*i).cloned()).collect();
                let edges = g
                    .edges()
                    .filter(|e| e.from == id && ids.contains(&e.to))
                    .cloned()
                    .collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    ..Default::default()
                })
            }
            Query::Neighbours {
                node,
                depth,
                direction,
            } => {
                let (ids, edges) = g.neighbours(node, depth, direction);
                let nodes = ids.iter().filter_map(|i| g.node(*i).cloned()).collect();
                Ok(QueryResult {
                    nodes,
                    edges,
                    ..Default::default()
                })
            }
            Query::All { limit, kinds } => {
                let (nodes, edges, truncated) = g.all(limit, kinds.as_deref());
                Ok(QueryResult {
                    nodes,
                    edges,
                    table: None,
                    truncated,
                })
            }
            Query::Text { dialect, text } if dialect == "search" => {
                drop(g);
                let hits = self.run_search(text.trim(), 20).await?;
                let g = self.graph.read().unwrap();
                let mut nodes: Vec<Node> = Vec::new();
                for h in &hits {
                    if !nodes.iter().any(|n| n.id == h.file) {
                        if let Some(n) = g.node(h.file) {
                            nodes.push(n.clone());
                        }
                    }
                }
                Ok(QueryResult {
                    nodes,
                    table: Some(Table {
                        columns: vec![
                            "path".into(),
                            "line".into(),
                            "score".into(),
                            "snippet".into(),
                        ],
                        rows: hits
                            .iter()
                            .map(|h| {
                                vec![
                                    Value::Text(h.path.clone()),
                                    Value::Int(h.line as i64),
                                    Value::Float(((h.score * 1000.0).round() / 1000.0) as f64),
                                    Value::Text(h.snippet.clone()),
                                ]
                            })
                            .collect(),
                        truncated: false,
                    }),
                    ..Default::default()
                })
            }
            Query::Text { dialect, .. } => Err(SourceError::Unsupported(format!(
                "dialect {dialect}; the index speaks `search`"
            ))),
        }
    }

    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        // Mirrored file nodes read through the folder; derived nodes have no body.
        let owner = self
            .graph
            .read()
            .unwrap()
            .node(node)
            .map(|n| n.source.clone())
            .ok_or(SourceError::NotFound)?;
        if owner == self.folder.id() {
            self.folder.fetch_text(node).await
        } else {
            Err(SourceError::Unsupported(
                "derived nodes have no text".into(),
            ))
        }
    }

    async fn apply(&self, _tx: Transaction) -> Result<Applied, SourceError> {
        Err(SourceError::Unsupported(
            "the index is read-only; write through the folder".into(),
        ))
    }

    async fn refresh(&self, node: NodeId) -> Result<(), SourceError> {
        let file = match self.folder.query(Query::Node(node)).await {
            Ok(r) => r.nodes.into_iter().next().ok_or(SourceError::NotFound)?,
            Err(e) => return Err(e),
        };
        self.graph.write().unwrap().insert_node(file.clone());
        if walk::wants_text(&file, &self.limits) {
            let (text, _) = self.folder.fetch_text(node).await?;
            self.extract_into(&file, &text);
        }
        let g = self.graph.read().unwrap();
        let mut s = self.stats.write().unwrap();
        s.links = g.count_edge_kind(&moonkale_core::EdgeKind::Links);
        s.symbols = g.count_kind(&NodeKind::Symbol);
        Ok(())
    }
}
