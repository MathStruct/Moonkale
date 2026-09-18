//! The renderer's own graph: what the host sends, plus positions.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InNode {
    pub id: String,
    pub label: String,
    /// `"file" | "directory" | "page" | "symbol" | other` — picks the colour.
    pub kind: String,
    #[serde(default)]
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InEdge {
    /// Indices into `nodes`.
    pub a: usize,
    pub b: usize,
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InGraph {
    pub nodes: Vec<InNode>,
    pub edges: Vec<InEdge>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub key: String,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub color: [f32; 4],
    pub degree: u32,
    /// Pinned by the user (being dragged): the layout leaves it alone.
    pub pinned: bool,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

pub fn color_for(kind: &str) -> [f32; 4] {
    match kind {
        "file" => [0.43, 0.66, 1.0, 1.0],    // blue
        "page" => [0.79, 0.65, 0.37, 1.0],   // amber: phantom / unresolved
        "symbol" => [0.42, 0.70, 0.55, 1.0], // green
        "directory" => [0.50, 0.53, 0.58, 1.0],
        "table" => [0.80, 0.50, 0.75, 1.0],
        "database" => [0.90, 0.45, 0.45, 1.0],
        "column" => [0.60, 0.60, 0.80, 1.0],
        "vertex" => [0.95, 0.60, 0.30, 1.0], // orange: graph-database data
        _ => [0.64, 0.66, 0.72, 1.0],
    }
}

pub fn edge_color(kind: &str) -> [f32; 4] {
    match kind {
        "links" => [0.79, 0.65, 0.37, 0.55],
        "defines" | "contains" => [0.45, 0.48, 0.55, 0.35],
        "custom" => [0.95, 0.60, 0.30, 0.6], // a named relation (Cypher rel table)
        _ => [0.55, 0.58, 0.65, 0.4],
    }
}

impl Graph {
    /// Build from host input; positions start on a deterministic spiral so
    /// the layout converges the same way every time for the same graph.
    pub fn from_input(input: InGraph) -> Self {
        let n = input.nodes.len().max(1) as f32;
        let mut degree = vec![0u32; input.nodes.len()];
        for e in &input.edges {
            if e.a < degree.len() && e.b < degree.len() {
                degree[e.a] += 1;
                degree[e.b] += 1;
            }
        }
        let spread = 12.0 * n.sqrt();
        let nodes = input
            .nodes
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                let t = i as f32 * 2.399_963; // golden angle
                let r = spread * ((i as f32 + 1.0) / (degree.len() as f32 + 1.0)).sqrt();
                let deg = degree[i];
                Node {
                    radius: 4.0 + (deg as f32).sqrt().min(6.0),
                    color: color_for(&n.kind),
                    id: n.id,
                    label: n.label,
                    kind: n.kind,
                    key: n.key,
                    x: r * t.cos(),
                    y: r * t.sin(),
                    degree: deg,
                    pinned: false,
                }
            })
            .collect();
        let edges = input
            .edges
            .into_iter()
            .filter(|e| e.a < degree.len() && e.b < degree.len() && e.a != e.b)
            .map(|e| Edge {
                a: e.a,
                b: e.b,
                color: edge_color(&e.kind),
            })
            .collect();
        Self { nodes, edges }
    }

    /// Axis-aligned bounds of all nodes (world units).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut it = self.nodes.iter();
        let first = it.next()?;
        let mut b = (first.x, first.y, first.x, first.y);
        for n in it {
            b.0 = b.0.min(n.x);
            b.1 = b.1.min(n.y);
            b.2 = b.2.max(n.x);
            b.3 = b.3.max(n.y);
        }
        Some(b)
    }
}
