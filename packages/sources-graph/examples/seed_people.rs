//! Creates a small LadybugDB database to try the Ladybug source with:
//!
//! ```sh
//! cargo run -p moonkale-sources-graph --features ladybug --example seed_people -- ~/moonkale-sample/people.lbug
//! ```
//!
//! Then open the containing folder in Moonkale and click `people.lbug`.
//! Database files are tied to the `lbug` storage version, so downloading one
//! from elsewhere is unlikely to open; regenerate instead.

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "people.lbug".to_string());
    let path = std::path::PathBuf::from(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directory");
    }
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::remove_file(&path);
    let db = lbug::Database::new(&path, lbug::SystemConfig::default()).expect("open database");
    let c = lbug::Connection::new(&db).expect("connection");
    for q in [
        "CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))",
        "CREATE NODE TABLE City(name STRING, country STRING, PRIMARY KEY(name))",
        "CREATE NODE TABLE Project(title STRING, year INT64, PRIMARY KEY(title))",
        "CREATE REL TABLE Knows(FROM Person TO Person, since INT64)",
        "CREATE REL TABLE LivesIn(FROM Person TO City)",
        "CREATE REL TABLE WorksOn(FROM Person TO Project, role STRING)",
        "CREATE (:Person {name:'Alice', age:30}), (:Person {name:'Bob', age:25}), (:Person {name:'Carol', age:41}), (:Person {name:'Dave', age:35})",
        "CREATE (:City {name:'Berlin', country:'DE'}), (:City {name:'Paris', country:'FR'}), (:City {name:'Zürich', country:'CH'})",
        "CREATE (:Project {title:'Moonkale', year:2026}), (:Project {title:'Quartz site', year:2025})",
        "MATCH (a:Person {name:'Alice'}), (b:Person {name:'Bob'}) CREATE (a)-[:Knows {since:2020}]->(b)",
        "MATCH (a:Person {name:'Bob'}), (b:Person {name:'Carol'}) CREATE (a)-[:Knows {since:2018}]->(b)",
        "MATCH (a:Person {name:'Carol'}), (b:Person {name:'Dave'}) CREATE (a)-[:Knows {since:2023}]->(b)",
        "MATCH (a:Person {name:'Dave'}), (b:Person {name:'Alice'}) CREATE (a)-[:Knows {since:2021}]->(b)",
        "MATCH (p:Person {name:'Alice'}), (c:City {name:'Berlin'}) CREATE (p)-[:LivesIn]->(c)",
        "MATCH (p:Person {name:'Bob'}), (c:City {name:'Berlin'}) CREATE (p)-[:LivesIn]->(c)",
        "MATCH (p:Person {name:'Carol'}), (c:City {name:'Paris'}) CREATE (p)-[:LivesIn]->(c)",
        "MATCH (p:Person {name:'Dave'}), (c:City {name:'Zürich'}) CREATE (p)-[:LivesIn]->(c)",
        "MATCH (p:Person {name:'Alice'}), (x:Project {title:'Moonkale'}) CREATE (p)-[:WorksOn {role:'lead'}]->(x)",
        "MATCH (p:Person {name:'Dave'}), (x:Project {title:'Moonkale'}) CREATE (p)-[:WorksOn {role:'graphics'}]->(x)",
        "MATCH (p:Person {name:'Carol'}), (x:Project {title:'Quartz site'}) CREATE (p)-[:WorksOn {role:'author'}]->(x)",
    ] {
        c.query(q).unwrap_or_else(|e| panic!("{q}: {e}"));
    }
    println!("wrote {}", path.display());
    println!("try:  MATCH (a:Person)-[r]->(b) RETURN a, r, b");
}
