//! The renderer's model without a browser (Milestone 18 phase 6.1): build a
//! graph, let the force layout settle, fit the camera, then ask what is
//! under a few screen points — what a non-Moonkale host (a Lenticulum
//! factor-graph viewer, say) does before it draws.
//!
//!     cargo run -p moonkale-graph-render --example headless

use moonkale_graph_render::graph::{InEdge, InGraph, InNode};
use moonkale_graph_render::scene::{Event, Scene};

fn main() {
    // A small factor graph: variables x1..x4, factors f1..f3.
    let var = |id: &str| InNode {
        id: id.into(),
        label: id.into(),
        kind: "variable".into(),
        key: id.into(),
        color: Some("#6ea8fe".into()),
    };
    let factor = |id: &str| InNode {
        id: id.into(),
        label: id.into(),
        kind: "factor".into(),
        key: id.into(),
        color: Some("#c9a75f".into()),
    };
    let nodes = vec![
        var("x1"),
        var("x2"),
        var("x3"),
        var("x4"),
        factor("f1"),
        factor("f2"),
        factor("f3"),
    ];
    let edge = |a, b| InEdge {
        a,
        b,
        kind: "connects".into(),
        color: None,
        ..Default::default()
    };
    let edges = vec![
        edge(4, 0),
        edge(4, 1),
        edge(5, 1),
        edge(5, 2),
        edge(6, 2),
        edge(6, 3),
    ];

    let mut scene = Scene::new(InGraph { nodes, edges });
    scene.resize(800.0, 600.0);
    let steps = scene.settle(2_000);
    scene.fit();
    println!("layout settled after {steps} steps");
    for n in &scene.graph.nodes {
        let (x, y) = scene.screen_position(&n.id).unwrap();
        println!(
            "{:>3} ({:>8}) at ({x:6.1}, {y:6.1}), degree {}",
            n.id, n.kind, n.degree
        );
    }
    let (x, y) = scene.screen_position("f2").unwrap();
    match scene.pointer_move(x, y) {
        Some(Event::Hover {
            id: Some(id),
            node_kind,
            ..
        }) => {
            println!("hover → {id} ({})", node_kind.unwrap_or_default())
        }
        other => println!("hover → {other:?}"),
    }
    println!(
        "click → {}",
        serde_json::to_string(&scene.click(x, y, false)).unwrap()
    );
}
