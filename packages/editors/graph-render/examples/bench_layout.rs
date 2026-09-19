//! Layout step timings on synthetic graphs (Milestone 6 measurements):
//! `cargo run --release -p moonkale-graph-render --example bench_layout`
use moonkale_graph_render::graph::{Graph, InEdge, InGraph, InNode};
use moonkale_graph_render::layout::Layout;
use std::time::Instant;

fn synthetic(n: usize) -> Graph {
    // A forest of stars plus random cross links: roughly what a folder index looks like.
    let nodes = (0..n)
        .map(|i| InNode {
            id: i.to_string(),
            label: format!("n{i}"),
            kind: if i % 7 == 0 {
                "directory".into()
            } else {
                "file".into()
            },
            key: String::new(),
            color: None,
        })
        .collect();
    let mut edges = Vec::new();
    for i in 1..n {
        edges.push(InEdge {
            a: i,
            b: i / 7,
            kind: "contains".into(),
            color: None,
        });
        if i % 5 == 0 {
            edges.push(InEdge {
                a: i,
                b: (i * 7919) % n,
                kind: "links".into(),
                color: None,
            });
        }
    }
    Graph::from_input(InGraph { nodes, edges })
}

fn main() {
    for &n in &[1_000usize, 10_000, 50_000, 100_000] {
        let mut g = synthetic(n);
        let mut layout = Layout::new(&g);
        let t0 = Instant::now();
        let steps = 10;
        for _ in 0..steps {
            layout.step(&mut g);
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / steps as f64;
        println!(
            "{n:>7} nodes {:>7} edges: {per:8.1} ms / step  → {:5.1} steps/s",
            g.edges.len(),
            1000.0 / per
        );
    }
}
