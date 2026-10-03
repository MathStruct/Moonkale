//! Milestone 17: a Turso database as a source — tables, columns, read-only
//! SQL, the schema graph; writes refused like SQLite's.
#![cfg(feature = "turso")]

use moonkale_core::{NodeKind, Query, Source, Value};
use moonkale_sources_sql::TursoSource;

#[tokio::test]
async fn turso_tables_columns_and_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shop.turso");
    {
        let db = turso::Builder::new_local(&path.to_string_lossy())
            .build()
            .await
            .unwrap();
        let c = db.connect().unwrap();
        c.execute(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
            (),
        )
        .await
        .unwrap();
        c.execute(
            "INSERT INTO users VALUES (1, 'ada', 3.5), (2, 'alan', 2.0)",
            (),
        )
        .await
        .unwrap();
        c.execute(
            "CREATE VIEW good AS SELECT name FROM users WHERE score > 3",
            (),
        )
        .await
        .unwrap();
    }
    let src = TursoSource::open(&path).await.unwrap();
    let d = src.descriptor();
    assert_eq!(d.display_name, "shop.turso");
    assert!(d.id.as_str().starts_with("turso:"));

    let tables = src.query(Query::Children(d.root)).await.unwrap();
    let labels: Vec<_> = tables.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["good (view)", "users"]);
    let users = tables.nodes.iter().find(|n| n.label == "users").unwrap();
    let cols = src.query(Query::Children(users.id)).await.unwrap();
    let cols: Vec<_> = cols.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(cols, ["id: INTEGER", "name: TEXT", "score: REAL"]);

    let r = src
        .query(Query::Text {
            dialect: "sql".into(),
            text: "SELECT name, score FROM users ORDER BY id".into(),
        })
        .await
        .unwrap();
    let t = r.table.unwrap();
    assert_eq!(t.columns, ["name", "score"]);
    assert_eq!(t.rows[0], [Value::Text("ada".into()), Value::Float(3.5)]);
    assert_eq!(t.rows.len(), 2);

    // Read-only, like SQLite: a write is refused before it reaches Turso.
    assert!(src
        .query(Query::Text {
            dialect: "sql".into(),
            text: "DELETE FROM users".into(),
        })
        .await
        .is_err());

    let g = src
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    assert!(g.nodes.iter().any(|n| n.kind == NodeKind::Database));
    assert_eq!(
        g.nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Column)
            .count(),
        4
    );
}
