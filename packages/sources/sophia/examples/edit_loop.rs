//! Moonkale's edit loop through Sophia, timed from outside (Sophia B-15):
//! open a name, save an edit through `Source::apply`, read the name's new
//! definition and the definitions that still use the old one.
//!
//! `cargo run --release -p moonkale-sources-sophia --example edit_loop --
//! [--store <file.sophia>] [--name <a.b.f>] [--text <file>] [--runs <n>]`.
//! Without `--store` it runs on SPEC-004's sample in memory, editing
//! `sample.add`; with `--store`, `--text` is the edited text. Each run alternates the original and the edited text, so
//! the store stays the same size. Prints one TSV line per run (µs per step)
//! and the median of each step at the end.
use moonkale_core::{Direction, EdgeKind, NodeId, OpResult, Query, Source, TextPatch, Transaction};
use moonkale_sources_sophia::SophiaSource;
use std::time::Instant;

async fn names_target(s: &SophiaSource, name: NodeId) -> NodeId {
    let g = s
        .query(Query::Neighbours {
            node: name,
            depth: 1,
            direction: Direction::Out,
        })
        .await
        .expect("neighbours of the name");
    g.edges
        .iter()
        .find(|e| e.from == name && e.kind == EdgeKind::Custom("Names".into()))
        .map(|e| e.to)
        .expect("a Names edge")
}

#[tokio::main]
async fn main() {
    let mut store = None;
    let mut name = "sample.add".to_string();
    let mut edited = None;
    let mut runs = 100usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--store" => store = Some(value()),
            "--name" => name = value(),
            "--text" => edited = Some(std::fs::read_to_string(value()).expect("--text file")),
            "--runs" => runs = value().parse().expect("--runs <n>"),
            _ => panic!("unknown argument {a}"),
        }
    }

    let t = Instant::now();
    let s = match &store {
        Some(path) => SophiaSource::open(path).await.expect("open the store"),
        None => SophiaSource::sample().expect("the sample"),
    };
    eprintln!("open\t{} µs", t.elapsed().as_micros());

    // Open: the Explorer resolves a path, the editor fetches its text.
    let t = Instant::now();
    let found = s
        .query(Query::Text {
            dialect: "path".into(),
            text: format!("name:{name}"),
        })
        .await
        .expect("the name");
    let node = found.nodes[0].id;
    let (original, _) = s.fetch_text(node).await.expect("its text");
    eprintln!("resolve+fetch\t{} µs", t.elapsed().as_micros());
    // The default edit, for the sample's `add`: a different definition of
    // the same function. A comment alone would keep the id.
    let edited = edited.unwrap_or_else(|| {
        assert!(store.is_none(), "--text is needed with --store");
        original.replace("Nat.succ(add(k, b))", "add(k, Nat.succ(b))")
    });
    assert_ne!(edited, original, "the edit changes nothing");

    println!("run\tfetch_us\tapply_us\tnew_id_us\tdependants_us\tdependants");
    let mut steps: [Vec<u128>; 4] = Default::default();
    for run in 0..runs {
        let text = if run % 2 == 0 { &edited } else { &original };
        let old = names_target(&s, node).await;

        let t0 = Instant::now();
        let (current, version) = s.fetch_text(node).await.expect("fetch");
        let t1 = Instant::now();
        let tx = Transaction::write_text(
            node,
            version,
            TextPatch::whole(text, current.chars().count()),
        );
        match s.apply(tx).await.expect("apply").results.remove(0) {
            OpResult::Ok { .. } => {}
            OpResult::Refused { error, .. } => panic!("the edit was refused: {error}"),
        }
        let t2 = Instant::now();
        let new = names_target(&s, node).await;
        let t3 = Instant::now();
        // Dependants keep referencing the old definition (content ids).
        let users = s
            .query(Query::Neighbours {
                node: old,
                depth: 1,
                direction: Direction::In,
            })
            .await
            .expect("dependants");
        let t4 = Instant::now();
        let dependants = users
            .edges
            .iter()
            .filter(|e| e.to == old && e.kind == EdgeKind::References)
            .count();
        assert_ne!(new, old, "the name moves to the new definition");

        let d = [t1 - t0, t2 - t1, t3 - t2, t4 - t3].map(|d| d.as_micros());
        println!(
            "{run}\t{}\t{}\t{}\t{}\t{dependants}",
            d[0], d[1], d[2], d[3]
        );
        for (all, x) in steps.iter_mut().zip(d) {
            all.push(x);
        }
    }
    let median = |v: &mut Vec<u128>| {
        v.sort_unstable();
        v[v.len() / 2]
    };
    let [a, b, c, d] = &mut steps;
    eprintln!(
        "median µs: fetch {}  apply {}  new id {}  dependants {}",
        median(a),
        median(b),
        median(c),
        median(d)
    );
}
