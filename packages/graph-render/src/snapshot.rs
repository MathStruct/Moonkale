//! Bounded overview geometry from existing positions; no layout or GPU readback.
use crate::graph::{Graph, Shape};

/// A thumbnail node in CSS pixels.
pub struct Dot {
    pub x: f64,
    pub y: f64,
    pub colour: [f32; 4],
    pub rect: bool,
}
/// At most 256 nodes and 512 edges, independent of graph size.
pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub nodes: Vec<Dot>,
    pub edges: Vec<(usize, usize)>,
    pub total_nodes: usize,
    pub total_edges: usize,
}
/// Project a bounded prefix. Invalid positions are omitted; extreme finite
/// coordinates use f64 intermediates so thumbnail coordinates remain finite.
pub fn snapshot(graph: &Graph, width: u32, height: u32) -> Snapshot {
    let width = width.clamp(1, 512);
    let height = height.clamp(1, 256);
    let mut indices = std::collections::HashMap::new();
    let mut nodes = Vec::new();
    for (i, n) in graph.nodes.iter().take(256).enumerate() {
        if !n.x.is_finite() || !n.y.is_finite() {
            continue;
        }
        indices.insert(i, nodes.len());
        nodes.push(Dot {
            x: n.x as f64,
            y: n.y as f64,
            colour: n.color,
            rect: n.shape != Shape::Circle,
        });
    }
    let min_x = nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let min_y = nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
    let max_x = nodes.iter().map(|n| n.x).fold(f64::NEG_INFINITY, f64::max);
    let max_y = nodes.iter().map(|n| n.y).fold(f64::NEG_INFINITY, f64::max);
    let scale = ((width.saturating_sub(12)) as f64 / (max_x - min_x).max(1.))
        .min((height.saturating_sub(12)) as f64 / (max_y - min_y).max(1.));
    for n in &mut nodes {
        n.x = width as f64 / 2. + (n.x - (min_x + max_x) / 2.) * scale;
        n.y = height as f64 / 2. + (n.y - (min_y + max_y) / 2.) * scale;
    }
    let edges = graph
        .edges
        .iter()
        .take(512)
        .filter_map(|e| Some((*indices.get(&e.a)?, *indices.get(&e.b)?)))
        .collect();
    Snapshot {
        width,
        height,
        nodes,
        edges,
        total_nodes: graph.nodes.len(),
        total_edges: graph.edges.len(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::InGraph;
    #[test]
    fn snapshot_bounds_work_and_extreme_positions() {
        let input = format!(
            "{{\"nodes\":[{}],\"edges\":[]}}",
            (0..1000)
                .map(|i| format!(
                    "{{\"id\":\"{i}\",\"label\":\"n\",\"kind\":\"file\",\"pos\":[{},0]}}",
                    if i % 2 == 0 { f32::MAX } else { -f32::MAX }
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let graph = Graph::from_input(serde_json::from_str::<InGraph>(&input).unwrap());
        let shot = snapshot(&graph, u32::MAX, 0);
        assert_eq!((shot.width, shot.height), (512, 1));
        assert_eq!(shot.nodes.len(), 256);
        assert_eq!(shot.total_nodes, 1000);
        assert!(shot
            .nodes
            .iter()
            .all(|n| n.x.is_finite() && n.y.is_finite()));
    }
}
