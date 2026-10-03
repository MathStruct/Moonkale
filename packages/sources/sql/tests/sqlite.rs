#![cfg(feature = "sqlite")]
use moonkale_core::{NodeKind, Query, Source, SourceError, Value};
use moonkale_sources_sql::SqliteSource;

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let c = rusqlite::Connection::open(dir.path().join("t.sqlite")).unwrap();
    c.execute_batch(
        "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, score REAL);
         INSERT INTO users(name, score) VALUES ('ann', 1.5), ('bob', NULL);
         CREATE VIEW top AS SELECT name FROM users ORDER BY score DESC;",
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn lists_tables_and_columns_as_a_graph() {
    let dir = fixture();
    let s = SqliteSource::open(dir.path().join("t.sqlite")).unwrap();
    let tables = s.query(Query::Children(s.root_id())).await.unwrap();
    let names: Vec<_> = tables.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(names, ["top (view)", "users"]);
    let users = tables
        .nodes
        .iter()
        .find(|n| n.native_key == "table:users")
        .unwrap();
    let cols = s.query(Query::Children(users.id)).await.unwrap();
    let labels: Vec<_> = cols.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["id: INTEGER", "name: TEXT", "score: REAL"]);
    assert!(cols.nodes.iter().all(|n| n.kind == NodeKind::Column));
    let all = s
        .query(Query::All {
            limit: 100,
            kinds: None,
        })
        .await
        .unwrap();
    assert_eq!(
        all.nodes.len(),
        1 + 2 + 3 + 1,
        "db + tables + users' columns + view column"
    );
}

#[tokio::test]
async fn read_queries_return_rows_and_writes_are_refused() {
    let dir = fixture();
    let s = SqliteSource::open(dir.path().join("t.sqlite")).unwrap();
    let res = s
        .query(Query::Text {
            dialect: "sql".into(),
            text: "SELECT id, name, score FROM users ORDER BY id".into(),
        })
        .await
        .unwrap();
    let t = res.table.unwrap();
    assert_eq!(t.columns, ["id", "name", "score"]);
    assert_eq!(
        t.rows[0],
        vec![Value::Int(1), Value::Text("ann".into()), Value::Float(1.5)]
    );
    assert_eq!(t.rows[1][2], Value::Null);
    assert!(!t.truncated);

    let err = s
        .query(Query::Text {
            dialect: "sql".into(),
            text: "DELETE FROM users".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, SourceError::Unsupported(_)));
    let bad = s
        .query(Query::Text {
            dialect: "sql".into(),
            text: "SELECT nope FROM users".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(bad, SourceError::Invalid(_)));
}
