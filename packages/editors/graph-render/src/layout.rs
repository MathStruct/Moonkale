//! Force-directed layout (Fruchterman–Reingold with cooling).
//!
//! Repulsion is exact O(n²) below [`BARNES_HUT_FROM`] nodes and Barnes–Hut
//! (quadtree, θ = 0.8) above — O(n log n), which is what makes 50k–100k
//! nodes possible on the CPU (Milestone 6, P-22).

use crate::graph::Graph;
use crate::quadtree::QuadTree;

/// Node count from which the quadtree approximation is used.
pub const BARNES_HUT_FROM: usize = 1500;
const THETA: f32 = 0.8;

pub struct Layout {
    pub temperature: f32,
    pub k: f32,
    pub running: bool,
    pub iterations: u32,
}

impl Layout {
    pub fn new(graph: &Graph) -> Self {
        let n = graph.nodes.len().max(1) as f32;
        // Ideal edge length grows slowly with graph size.
        let k = 28.0 + 6.0 * n.ln();
        Self {
            temperature: 10.0 * n.sqrt(),
            k,
            running: true,
            iterations: 0,
        }
    }

    /// One iteration. Returns the largest displacement, so callers can stop
    /// when the graph settles.
    pub fn step(&mut self, graph: &mut Graph) -> f32 {
        let n = graph.nodes.len();
        if n == 0 || !self.running {
            self.running = false;
            return 0.0;
        }
        let k = self.k;
        let k2 = k * k;
        let mut disp = vec![[0.0f32; 2]; n];

        if n >= BARNES_HUT_FROM {
            let points: Vec<(f32, f32)> = graph.nodes.iter().map(|nd| (nd.x, nd.y)).collect();
            let tree = QuadTree::build(&points);
            for (i, &(x, y)) in points.iter().enumerate() {
                let (fx, fy) = tree.force(i, x, y, k2, THETA);
                disp[i][0] += fx;
                disp[i][1] += fy;
            }
        }
        // Repulsion between every pair (small graphs: exact).
        for i in 0..n {
            if n >= BARNES_HUT_FROM {
                break;
            }
            let (xi, yi) = (graph.nodes[i].x, graph.nodes[i].y);
            for j in (i + 1)..n {
                let mut dx = xi - graph.nodes[j].x;
                let mut dy = yi - graph.nodes[j].y;
                let mut d2 = dx * dx + dy * dy;
                if d2 < 0.01 {
                    // Coincident: nudge deterministically.
                    dx = 0.1 * ((i as f32) + 1.0);
                    dy = 0.1 * ((j as f32) + 1.0);
                    d2 = dx * dx + dy * dy;
                }
                let d = d2.sqrt();
                let f = k2 / d; // repulsive force magnitude
                let (fx, fy) = (dx / d * f, dy / d * f);
                disp[i][0] += fx;
                disp[i][1] += fy;
                disp[j][0] -= fx;
                disp[j][1] -= fy;
            }
        }
        // Attraction along edges. Hubs pull less per edge ("dissuade hubs",
        // as in ForceAtlas2): a vault whose index page links to everything
        // would otherwise fold every note onto the line between its hubs
        // (spec 017 follow-up).
        for e in &graph.edges {
            let (a, b) = (e.a, e.b);
            let dx = graph.nodes[a].x - graph.nodes[b].x;
            let dy = graph.nodes[a].y - graph.nodes[b].y;
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let hub = graph.nodes[a].degree.max(graph.nodes[b].degree) as f32;
            let f = d * d / k / (1.0 + 0.5 * hub.ln_1p());
            let (fx, fy) = (dx / d * f, dy / d * f);
            disp[a][0] -= fx;
            disp[a][1] -= fy;
            disp[b][0] += fx;
            disp[b][1] += fy;
        }
        // Gravity towards the origin keeps disconnected pieces together;
        // isolated nodes feel it more, so they ring the graph instead of
        // flying off and stretching the fit.
        for (i, node) in graph.nodes.iter().enumerate() {
            let g = if node.degree == 0 { 0.12 } else { 0.03 };
            disp[i][0] -= node.x * g;
            disp[i][1] -= node.y * g;
        }
        // Move, capped by temperature.
        let mut max_move = 0.0f32;
        for (i, node) in graph.nodes.iter_mut().enumerate() {
            if node.pinned {
                continue;
            }
            let (dx, dy) = (disp[i][0], disp[i][1]);
            let d = (dx * dx + dy * dy).sqrt();
            if d > 0.0 {
                let m = d.min(self.temperature);
                node.x += dx / d * m;
                node.y += dy / d * m;
                max_move = max_move.max(m);
            }
        }
        self.temperature *= 0.95;
        self.iterations += 1;
        // Big graphs settle "well enough" much earlier; keep the UI alive.
        let max_iter = if n > 50_000 {
            120
        } else if n > 10_000 {
            250
        } else {
            600
        };
        if self.temperature < 0.3 || self.iterations > max_iter {
            self.running = false;
        }
        max_move
    }

    pub fn reheat(&mut self, graph: &Graph) {
        *self = Self::new(graph);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{InEdge, InGraph, InNode};

    /// Two hubs linked to every leaf (a vault's Home + index): the leaves
    /// must not collapse onto the hub–hub line.
    #[test]
    fn two_hubs_do_not_flatten_the_graph() {
        let leaves = 60;
        let mut nodes: Vec<InNode> = (0..leaves + 2)
            .map(|i| InNode {
                id: format!("n{i}"),
                label: format!("n{i}"),
                kind: "file".into(),
                key: String::new(),
                color: None,
            })
            .collect();
        nodes[0].id = "home".into();
        nodes[1].id = "index".into();
        let mut edges = Vec::new();
        for l in 2..leaves + 2 {
            edges.push(InEdge {
                a: 0,
                b: l,
                kind: String::new(),
                color: None,
            });
            edges.push(InEdge {
                a: 1,
                b: l,
                kind: String::new(),
                color: None,
            });
        }
        let mut g = Graph::from_input(InGraph { nodes, edges });
        let mut l = Layout::new(&g);
        let mut steps = 0;
        while l.running && steps < 1000 {
            l.step(&mut g);
            steps += 1;
        }
        let (x0, y0, x1, y1) = g.bounds().unwrap();
        // Extent orthogonal to the hub axis relative to the extent along it.
        let (hx, hy) = (g.nodes[1].x - g.nodes[0].x, g.nodes[1].y - g.nodes[0].y);
        let hl = (hx * hx + hy * hy).sqrt().max(1.0);
        let (ux, uy) = (hx / hl, hy / hl);
        let (mut along_min, mut along_max, mut across_min, mut across_max) =
            (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for n in &g.nodes {
            let along = n.x * ux + n.y * uy;
            let across = -n.x * uy + n.y * ux;
            along_min = along_min.min(along);
            along_max = along_max.max(along);
            across_min = across_min.min(across);
            across_max = across_max.max(across);
        }
        let ratio = (across_max - across_min) / (along_max - along_min).max(1.0);
        assert!(
            ratio > 0.5,
            "graph flattened: across/along = {ratio:.2}, bounds {x0:.0},{y0:.0}..{x1:.0},{y1:.0}"
        );
    }

    fn chain(n: usize) -> Graph {
        let nodes = (0..n)
            .map(|i| InNode {
                id: i.to_string(),
                label: i.to_string(),
                kind: "file".into(),
                key: String::new(),
                color: None,
            })
            .collect();
        let edges = (1..n)
            .map(|i| InEdge {
                a: i - 1,
                b: i,
                kind: "links".into(),
                color: None,
            })
            .collect();
        Graph::from_input(InGraph { nodes, edges })
    }

    #[test]
    fn layout_converges_and_separates_nodes() {
        let mut g = chain(30);
        let mut l = Layout::new(&g);
        let mut steps = 0;
        while l.running && steps < 1000 {
            l.step(&mut g);
            steps += 1;
        }
        assert!(!l.running, "did not settle");
        // No two nodes on top of each other.
        for i in 0..g.nodes.len() {
            for j in (i + 1)..g.nodes.len() {
                let (dx, dy) = (g.nodes[i].x - g.nodes[j].x, g.nodes[i].y - g.nodes[j].y);
                assert!(dx * dx + dy * dy > 1.0, "nodes {i} and {j} overlap");
            }
        }
    }

    #[test]
    fn pinned_nodes_stay_put() {
        let mut g = chain(5);
        g.nodes[0].pinned = true;
        let (x, y) = (g.nodes[0].x, g.nodes[0].y);
        let mut l = Layout::new(&g);
        for _ in 0..50 {
            l.step(&mut g);
        }
        assert_eq!((g.nodes[0].x, g.nodes[0].y), (x, y));
    }
}
