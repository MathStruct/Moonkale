//! What the Explorer and the graph panel ask of a store, timed: the root's
//! children, the largest namespace's children, `Query::All` at the graph
//! panel's limit, and one name's neighbours and text.
//!
//! `cargo run --release -p moonkale-sources-sophia --example browse --
//! <file.sophia>`.
use moonkale_core::{Direction, NodeKind, Query, Source};
use moonkale_sources_sophia::SophiaSource;
use std::time::Instant;

/// The graph panel's `GRAPH_LIMIT`.
const GRAPH_LIMIT: usize = 100_000;

#[tokio::main]
async fn main() {
    let path = std::env::args().nth(1).expect("a .sophia file");
    let t = Instant::now();
    let s = SophiaSource::open(&path).await.expect("open");
    println!("open\t{:?}", t.elapsed());

    let t = Instant::now();
    let root = s.query(Query::Children(s.descriptor().root)).await.unwrap();
    println!(
        "root children\t{:?}\t{} nodes",
        t.elapsed(),
        root.nodes.len()
    );

    // Walk namespaces breadth first and time each listing; keep the largest.
    let mut queue: Vec<_> = root
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Directory && n.label != "views")
        .map(|n| n.id)
        .collect();
    let (mut largest, mut slowest) = ((0, None), std::time::Duration::ZERO);
    let mut listed = 0;
    // Names whose text doesn't print, out of those tried (at most 5000).
    let (mut tried, mut failed, mut first_error) = (0, 0, None);
    while let Some(ns) = queue.pop() {
        if listed >= 200 {
            break;
        }
        let t = Instant::now();
        let r = s.query(Query::Children(ns)).await.unwrap();
        slowest = slowest.max(t.elapsed());
        listed += 1;
        if r.nodes.len() > largest.0 {
            largest = (
                r.nodes.len(),
                r.nodes
                    .iter()
                    .find(|n| n.kind != NodeKind::Directory)
                    .map(|n| n.id),
            );
        }
        queue.extend(
            r.nodes
                .iter()
                .filter(|n| n.kind == NodeKind::Directory)
                .map(|n| n.id),
        );
        for n in r.nodes.iter().filter(|n| n.kind != NodeKind::Directory) {
            if tried >= 5000 {
                break;
            }
            tried += 1;
            if let Err(e) = s.fetch_text(n.id).await {
                failed += 1;
                first_error.get_or_insert_with(|| format!("{}: {e}", n.native_key));
            }
        }
    }
    println!(
        "namespaces listed\t{listed}\tlargest {} children\tslowest {slowest:?}",
        largest.0
    );
    println!(
        "texts\t{failed} of {tried} failed\t{}",
        first_error.unwrap_or_default()
    );

    let t = Instant::now();
    let all = s
        .query(Query::All {
            limit: GRAPH_LIMIT,
            kinds: None,
        })
        .await
        .unwrap();
    println!(
        "all (graph panel)\t{:?}\t{} nodes\t{} edges\ttruncated {}",
        t.elapsed(),
        all.nodes.len(),
        all.edges.len(),
        all.truncated
    );
    let bytes = serde_json::to_vec(&all).map(|v| v.len()).unwrap_or(0);
    println!("all as JSON\t{} MB", bytes / 1_000_000);

    if let Some(name) = largest.1 {
        let t = Instant::now();
        let n = s
            .query(Query::Neighbours {
                node: name,
                depth: 1,
                direction: Direction::Both,
            })
            .await
            .unwrap();
        println!(
            "neighbours of a name\t{:?}\t{} nodes",
            t.elapsed(),
            n.nodes.len()
        );
        let t = Instant::now();
        match s.fetch_text(name).await {
            Ok((text, _)) => println!("fetch_text\t{:?}\t{} bytes", t.elapsed(), text.len()),
            Err(e) => println!("fetch_text\t{:?}\t{e}", t.elapsed()),
        }
    }
}
