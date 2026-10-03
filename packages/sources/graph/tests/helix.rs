//! Milestone 17: an embedded HelixDB store as a source — labels in the tree,
//! vertices and edges as a graph, the `helix` dialect.
#![cfg(feature = "helix")]
#![recursion_limit = "256"]

use helix_db::dsl::prelude::*;
use helix_db::{Client, HelixDbSource, QueryRequest};
use moonkale_core::{EdgeKind, NodeKind, Query, Source, Value};
use moonkale_sources_graph::HelixSource;

async fn fixture(root: &std::path::Path) {
    let client = Client::open(HelixDbSource::Disk {
        root: root.to_path_buf(),
        database: "main".into(),
    })
    .await
    .unwrap();
    let w = write_batch()
        .var_as("a", g().add_n("Person", vec![("name", "Ada")]))
        .var_as("b", g().add_n("Person", vec![("name", "Alan")]))
        .var_as("c", g().add_n("City", vec![("name", "London")]))
        .var_as(
            "k",
            g().n(NodeRef::var("a"))
                .add_e("KNOWS", NodeRef::var("b"), vec![("since", "1843")])
                .count(),
        )
        .var_as(
            "l",
            g().n(NodeRef::var("a"))
                .add_e("LIVES_IN", NodeRef::var("c"), Vec::<(&str, &str)>::new())
                .count(),
        )
        .returning(["a"]);
    let _: serde_json::Value = client.query(QueryRequest::write(w)).send().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn helix_labels_graph_and_queries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("people.helix");
    std::fs::create_dir_all(&root).unwrap();
    fixture(&root).await;
    assert!(moonkale_sources_graph::is_helix_path(
        &root.to_string_lossy()
    ));

    let src = HelixSource::open(&root).await.unwrap();
    let d = src.descriptor();
    assert_eq!(d.display_name, "people.helix");

    let tree = src.query(Query::Children(d.root)).await.unwrap();
    let labels: Vec<_> = tree.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "City · 1",
            "Person · 2",
            "KNOWS (edge) · 1",
            "LIVES_IN (edge) · 1"
        ]
    );
    assert!(tree.nodes.iter().all(|n| n.kind == NodeKind::Table));

    let all = src
        .query(Query::All {
            limit: 100,
            kinds: Some(vec![NodeKind::Vertex]),
        })
        .await
        .unwrap();
    let mut names: Vec<_> = all.nodes.iter().map(|n| n.label.clone()).collect();
    names.sort();
    assert_eq!(names, ["Ada", "Alan", "London"]);
    assert!(all
        .nodes
        .iter()
        .any(|n| n.native_key.starts_with("v:Person:")));
    let mut kinds: Vec<_> = all
        .edges
        .iter()
        .map(|e| match &e.kind {
            EdgeKind::Custom(l) => l.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    kinds.sort();
    assert_eq!(kinds, ["KNOWS", "LIVES_IN"]);

    let r = src
        .query(Query::Text {
            dialect: "helix".into(),
            text: "nodes Person limit 10".into(),
        })
        .await
        .unwrap();
    let t = r.table.unwrap();
    assert_eq!(t.columns, ["$id", "$label", "name"]);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(r.nodes.len(), 2, "Show in Graph gets the vertices too");

    let r = src
        .query(Query::Text {
            dialect: "helix".into(),
            text: "edges KNOWS".into(),
        })
        .await
        .unwrap();
    let t = r.table.unwrap();
    assert_eq!(t.columns, ["$id", "$label", "$from", "$to", "since"]);
    assert_eq!(t.rows[0][4], Value::Text("1843".into()));
    assert_eq!(r.edges.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helix_root_without_a_database_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("empty.helix");
    std::fs::create_dir_all(&root).unwrap();
    assert!(HelixSource::open(&root).await.is_err());
}
