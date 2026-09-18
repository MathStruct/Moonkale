//! Integration test on a temp database: schema graph, Cypher → table + nodes/edges.
#![cfg(feature = "ladybug")]

use moonkale_core::{NodeKind, Query, Source};
use moonkale_sources_graph::ladybug::LadybugSource;

fn seed(dir: &std::path::Path) {
    let db = lbug::Database::new(dir, lbug::SystemConfig::default()).unwrap();
    let c = lbug::Connection::new(&db).unwrap();
    c.query("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();
    c.query("CREATE REL TABLE Knows(FROM Person TO Person, since INT64)")
        .unwrap();
    c.query("CREATE (:Person {name:'Alice', age:30})-[:Knows {since:2020}]->(:Person {name:'Bob', age:25})").unwrap();
}

#[tokio::test]
async fn schema_and_cypher() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("people.lbug");
    seed(&dir);
    let src = LadybugSource::open(&dir).unwrap();
    assert_eq!(src.descriptor().display_name, "people.lbug");

    let schema = src
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    let names: Vec<_> = schema.nodes.iter().map(|n| n.label.as_str()).collect();
    assert!(names.contains(&"Person"), "{names:?}");
    assert!(
        names.iter().any(|n| n.starts_with("name: STRING")),
        "{names:?}"
    );
    assert!(schema
        .edges
        .iter()
        .any(|e| matches!(&e.kind, moonkale_core::EdgeKind::Custom(k) if k == "Knows")));

    let kids = src.query(Query::Children(src.root_id())).await.unwrap();
    assert!(kids.nodes.iter().all(|n| n.kind == NodeKind::Table));

    let r = src
        .query(Query::Text {
            dialect: "cypher".into(),
            text: "MATCH (a:Person)-[k:Knows]->(b:Person) RETURN a, k, b, a.name".into(),
        })
        .await
        .unwrap();
    let t = r.table.unwrap();
    assert_eq!(t.columns, vec!["a", "k", "b", "a.name"]);
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.rows[0][3].to_string(), "Alice");
    assert_eq!(r.nodes.len(), 2);
    assert!(r.nodes.iter().all(|n| n.kind == NodeKind::Vertex));
    assert_eq!(r.edges.len(), 1);

    // One hop around Alice via Children.
    let alice = r.nodes.iter().find(|n| n.label.contains("Alice")).unwrap();
    let hop = src.query(Query::Children(alice.id)).await.unwrap();
    assert_eq!(hop.nodes.len(), 2);

    let err = src
        .query(Query::Text {
            dialect: "sql".into(),
            text: "select 1".into(),
        })
        .await;
    assert!(err.is_err());
}
