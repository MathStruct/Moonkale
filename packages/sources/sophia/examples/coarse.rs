//! Numeric-only coarse-query profile. Run on a disposable store copy: opening
//! Sophia may perform its own migrations/maintenance, as for the browse example.
use moonkale_core::{Query, Source, Value};
use moonkale_sources_sophia::SophiaSource;
use std::time::Instant;

#[tokio::main]
async fn main() {
    let path = std::env::args().nth(1).expect("disposable .sophia copy");
    let start = Instant::now();
    let source = SophiaSource::open(path).await.expect("open copied store");
    println!("open_ms={:.1}", start.elapsed().as_secs_f64() * 1000.0);
    for run in 0..3 {
        let start = Instant::now();
        let result = source
            .query(Query::Coarse {
                level: 1,
                region: None,
            })
            .await
            .expect("coarse query");
        println!(
            "run={run} coarse_ms={:.1} nodes={} edges={} source_nodes={}",
            start.elapsed().as_secs_f64() * 1000.0,
            result.nodes.len(),
            result.edges.len(),
            result.table.as_ref().unwrap().rows[0][0]
        );
        if run == 0 {
            if let Some(node)=result.nodes.iter().find(|n| matches!(n.props.get("coarse.members"),Some(Value::Int(count)) if *count>0 && *count<100000)) {
                let start=Instant::now();
                let region=source.query(Query::Coarse {level:1,region:Some(node.id)}).await.expect("region query");
                println!("region_ms={:.1} nodes={} edges={}",start.elapsed().as_secs_f64()*1000.0,region.nodes.len(),region.edges.len());
            }
        }
    }
}
