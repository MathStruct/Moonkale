//! `IndexGraph` — the in-memory derived graph.
//!
//! Nodes are either *mirrored* from the folder (files and directories, with
//! `source = folder id`) or *derived* (symbols, phantom pages for unresolved
//! links, `source = index id`). Edges are grouped by the file they were
//! derived from so a single file can be re-indexed cheaply.

use moonkale_core::{Direction, Edge, EdgeKind, Node, NodeId, NodeKind, SourceId};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct IndexGraph {
    pub nodes: HashMap<NodeId, Node>,
    /// Edges keyed by the file whose content produced them (`Contains`
    /// edges from the folder walk are keyed by the parent directory).
    edges_by_origin: HashMap<NodeId, Vec<Edge>>,
    /// Derived nodes (symbols, phantoms) produced from each file.
    derived_by_origin: HashMap<NodeId, Vec<NodeId>>,
    /// Lower-cased file stem → files, for wiki-link resolution.
    by_stem: HashMap<String, Vec<NodeId>>,
    /// Relative path → file, for markdown-link resolution.
    by_path: HashMap<String, NodeId>,
}

impl IndexGraph {
    pub fn insert_node(&mut self, node: Node) {
        if node.kind == NodeKind::File || node.kind == NodeKind::Directory {
            let key = node.native_key.clone();
            if node.kind == NodeKind::File {
                let stem = stem_of(&key).to_ascii_lowercase();
                let list = self.by_stem.entry(stem).or_default();
                if !list.contains(&node.id) {
                    list.push(node.id);
                }
            }
            self.by_path.insert(key, node.id);
        }
        self.nodes.insert(node.id, node);
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn file_by_path(&self, rel: &str) -> Option<NodeId> {
        self.by_path.get(rel).copied()
    }

    /// Obsidian's rule: the shortest path wins among files with that stem.
    pub fn file_by_stem(&self, stem: &str) -> Option<NodeId> {
        let list = self.by_stem.get(&stem.to_ascii_lowercase())?;
        list.iter().copied().min_by_key(|id| {
            self.nodes
                .get(id)
                .map(|n| n.native_key.len())
                .unwrap_or(usize::MAX)
        })
    }

    /// Replace everything derived from `origin` (edges + derived nodes).
    pub fn set_derived(&mut self, origin: NodeId, nodes: Vec<Node>, edges: Vec<Edge>) {
        if let Some(old) = self.derived_by_origin.remove(&origin) {
            for id in old {
                self.nodes.remove(&id);
            }
        }
        let ids: Vec<NodeId> = nodes.iter().map(|n| n.id).collect();
        for n in nodes {
            self.nodes.insert(n.id, n);
        }
        self.derived_by_origin.insert(origin, ids);
        self.edges_by_origin.insert(origin, edges);
    }

    /// Forget a mirrored file or directory and everything below it: the
    /// nodes, what was derived from them, and the `Contains` edges that
    /// pointed at them (Milestone 7: rename/delete).
    pub fn remove_subtree(&mut self, id: NodeId) -> Vec<NodeId> {
        let gone = self.subtree(id);
        for g in &gone {
            if let Some(old) = self.derived_by_origin.remove(g) {
                for d in old {
                    self.nodes.remove(&d);
                }
            }
            self.edges_by_origin.remove(g);
            if let Some(n) = self.nodes.remove(g) {
                self.by_path.remove(&n.native_key);
                if let Some(list) = self
                    .by_stem
                    .get_mut(&stem_of(&n.native_key).to_ascii_lowercase())
                {
                    list.retain(|x| x != g);
                }
            }
        }
        for edges in self.edges_by_origin.values_mut() {
            edges.retain(|e| !gone.contains(&e.to) && !gone.contains(&e.from));
        }
        gone
    }

    /// A mirrored node and every mirrored node below it (by path).
    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        let Some(root) = self.nodes.get(&id) else {
            return Vec::new();
        };
        let prefix = format!("{}/", root.native_key);
        self.nodes
            .values()
            .filter(|n| {
                n.id == id
                    || ((n.kind == NodeKind::File || n.kind == NodeKind::Directory)
                        && n.source == root.source
                        && n.native_key.starts_with(&prefix))
            })
            .map(|n| n.id)
            .collect()
    }

    /// Files whose derived edges point at any of `targets` (their links need
    /// re-resolving when a target appears or disappears).
    pub fn origins_linking_to(&self, targets: &[NodeId]) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self
            .edges_by_origin
            .iter()
            .filter(|(origin, edges)| {
                !targets.contains(origin) && edges.iter().any(|e| targets.contains(&e.to))
            })
            .map(|(o, _)| *o)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Phantom pages (unresolved links) that a file at `rel` would resolve.
    pub fn phantoms_for(&self, rel: &str) -> Vec<NodeId> {
        let stem = stem_of(rel).to_ascii_lowercase();
        let path = rel.to_ascii_lowercase();
        let no_ext = path
            .rsplit_once('.')
            .map(|(p, _)| p.to_string())
            .unwrap_or(path.clone());
        self.nodes
            .values()
            .filter(|n| n.kind == NodeKind::Page && n.native_key.starts_with("unresolved:"))
            .filter(|n| {
                let l = n.label.to_ascii_lowercase();
                l == stem || l == path || l == no_ext || no_ext.ends_with(&format!("/{l}"))
            })
            .map(|n| n.id)
            .collect()
    }

    /// Add one edge to an origin's set (idempotent).
    pub fn add_edge(&mut self, origin: NodeId, edge: Edge) {
        let list = self.edges_by_origin.entry(origin).or_default();
        if !list.contains(&edge) {
            list.push(edge);
        }
    }

    pub fn edges(&self) -> impl Iterator<Item = &Edge> {
        self.edges_by_origin.values().flatten()
    }

    pub fn edge_count(&self) -> usize {
        self.edges_by_origin.values().map(Vec::len).sum()
    }

    pub fn count_kind(&self, kind: &NodeKind) -> usize {
        self.nodes.values().filter(|n| &n.kind == kind).count()
    }

    pub fn count_edge_kind(&self, kind: &EdgeKind) -> usize {
        self.edges().filter(|e| &e.kind == kind).count()
    }

    /// Children of `id`: targets of its `Contains`/`Defines` edges.
    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.edges()
            .filter(|e| e.from == id && matches!(e.kind, EdgeKind::Contains | EdgeKind::Defines))
            .map(|e| e.to)
            .collect()
    }

    /// BFS neighbourhood within `depth` hops; returns node ids and the edges
    /// among them.
    pub fn neighbours(
        &self,
        start: NodeId,
        depth: u32,
        direction: Direction,
    ) -> (Vec<NodeId>, Vec<Edge>) {
        let mut seen: HashSet<NodeId> = HashSet::from([start]);
        let mut frontier = vec![start];
        for _ in 0..depth {
            let mut next = Vec::new();
            for e in self.edges() {
                let (a, b) = (e.from, e.to);
                let step = |from: NodeId,
                            to: NodeId,
                            next: &mut Vec<NodeId>,
                            seen: &mut HashSet<NodeId>| {
                    if frontier.contains(&from) && seen.insert(to) {
                        next.push(to);
                    }
                };
                match direction {
                    Direction::Out => step(a, b, &mut next, &mut seen),
                    Direction::In => step(b, a, &mut next, &mut seen),
                    Direction::Both => {
                        step(a, b, &mut next, &mut seen);
                        step(b, a, &mut next, &mut seen);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        let edges = self
            .edges()
            .filter(|e| seen.contains(&e.from) && seen.contains(&e.to))
            .cloned()
            .collect();
        (seen.into_iter().collect(), edges)
    }

    /// Every node (optionally of the given kinds) up to `limit`, plus the
    /// edges among the returned nodes. Files come first so a cap keeps the
    /// skeleton of the folder.
    pub fn all(&self, limit: usize, kinds: Option<&[NodeKind]>) -> (Vec<Node>, Vec<Edge>, bool) {
        let mut nodes: Vec<&Node> = self
            .nodes
            .values()
            .filter(|n| kinds.is_none_or(|ks| ks.contains(&n.kind)))
            .collect();
        nodes.sort_by_key(|n| (rank(&n.kind), n.native_key.clone()));
        let truncated = nodes.len() > limit;
        nodes.truncate(limit);
        let ids: HashSet<NodeId> = nodes.iter().map(|n| n.id).collect();
        let edges = self
            .edges()
            .filter(|e| ids.contains(&e.from) && ids.contains(&e.to))
            .cloned()
            .collect();
        (nodes.into_iter().cloned().collect(), edges, truncated)
    }
}

fn rank(kind: &NodeKind) -> u8 {
    match kind {
        NodeKind::Directory => 0,
        NodeKind::File => 1,
        NodeKind::Page => 2,
        NodeKind::Symbol => 3,
        _ => 4,
    }
}

/// `"docs/My Note.md"` → `"My Note"`.
pub fn stem_of(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Id of a derived node owned by the index.
pub fn derived_id(index: &SourceId, key: &str) -> NodeId {
    NodeId::derive(index, key)
}
