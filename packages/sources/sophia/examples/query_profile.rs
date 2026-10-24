//! Profile All(1000) on a disposable store copy (open may run maintenance).
//! Reports counts and timings only; no source bodies or stored values.
//! Run 0 warms up each phase; runs 1–5 are the measured samples.
//! Use MOONKALE_GRAPH_PROFILE=1 for the adapter/interface timing split.
use moonkale_core::{Query, Source};
use moonkale_sources_sophia::SophiaSource;
use sophia_interface::{Interface, Key};
use sophia_store::sqlite::Sqlite;
use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("disposable store copy required")?;
    let interface = Interface::open_read_only(Sqlite::open(&path)?, "moonkale");
    for run in 0..6 {
        let start = Instant::now();
        let graph = interface.all(1000)?;
        println!(
            "run={run} all_ms={:.3} nodes={} edges={} truncated={}",
            ms(start),
            graph.nodes.len(),
            graph.edges.len(),
            graph.truncated
        );
        for phase in ["names_under", "meta_prefix", "names_from"] {
            let start = Instant::now();
            let rows = interface.store().read(|tx| {
                let mut rows = 0;
                for node in &graph.nodes {
                    if let Key::Derived(key) = &node.key {
                        if let Some(ns) = key.strip_prefix("ns:") {
                            let base = if ns.is_empty() {
                                String::new()
                            } else {
                                format!("{ns}.")
                            };
                            rows += match phase {
                                "names_under" => tx.names_under(ns, 2)?.len(),
                                "meta_prefix" => tx.meta_prefix(&format!("ns/{base}"))?.len(),
                                _ => tx.names_from(&base, &base, 512)?.len(),
                            };
                        }
                    }
                }
                Ok(rows)
            })?;
            println!("run={run} phase={phase} ms={:.3} rows={rows}", ms(start));
        }
        let start = Instant::now();
        for node in &graph.nodes {
            std::hint::black_box(interface.node(&node.key)?);
        }
        println!("run={run} individual_nodes_ms={:.3}", ms(start));
        let start = Instant::now();
        let mut namespaces = 0;
        let mut children = 0;
        for node in &graph.nodes {
            if matches!(&node.key, Key::Derived(s) if s.starts_with("ns:")) {
                namespaces += 1;
                children += interface.children(&node.key)?.nodes.len();
            }
        }
        println!(
            "run={run} namespace_children_ms={:.3} namespaces={namespaces} children={children}",
            ms(start)
        );
    }
    let source = SophiaSource::open(&path).await?;
    for run in 0..6 {
        let start = Instant::now();
        let graph = source
            .query(Query::All {
                limit: 1000,
                kinds: None,
            })
            .await?;
        println!(
            "run={run} source_ms={:.3} nodes={} edges={}",
            ms(start),
            graph.nodes.len(),
            graph.edges.len()
        );
    }
    Ok(())
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
