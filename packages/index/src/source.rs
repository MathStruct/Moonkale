//! `IndexSource` — the derived graph as a `Source`.

use crate::extract::{symbols_rust, wikilinks, Derived};
use crate::graph::IndexGraph;
use crate::walk::{self, Limits};
use moonkale_core::{
    async_trait, Applied, Capabilities, ContentRef, Edge, Node, NodeId, NodeKind, Query,
    QueryResult, Source, SourceDescriptor, SourceError, SourceFamily, SourceId, Transaction,
    Version,
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
}

impl IndexSource {
    /// Build the index of `folder` now. The folder's own root node becomes
    /// the index root, so the index's `Children(root)` is the folder's tree
    /// enriched with derived nodes.
    pub async fn build(folder: Arc<dyn Source>) -> Result<Self, SourceError> {
        let fd = folder.descriptor();
        let id = SourceId::new(format!("index:{}", fd.id));
        let this = Self {
            id,
            root: fd.root,
            folder,
            graph: RwLock::new(IndexGraph::default()),
            stats: RwLock::new(IndexStats::default()),
            limits: Limits::default(),
        };
        this.rebuild().await?;
        Ok(this)
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
            Query::Text { .. } => Err(SourceError::Unsupported(
                "the index has no query language yet".into(),
            )),
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
