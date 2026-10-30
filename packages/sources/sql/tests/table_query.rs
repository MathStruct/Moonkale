//! Generated fixtures and brute-force reference; no user databases.
use moonkale_core::{
    Source, TableCursor, TableFilter, TablePredicate as P, TableQuery, TableSort, Value,
};
use std::cmp::Ordering;

fn seed() -> String {
    let mut sql = "CREATE TABLE items(id INTEGER PRIMARY KEY, g INTEGER, label TEXT);".to_string();
    for i in 0..47 {
        let g = if i % 7 == 0 {
            "NULL".into()
        } else {
            (i % 4).to_string()
        };
        let text = if i == 11 {
            "a%_''; DROP TABLE items; --".into()
        } else {
            format!("word{}", i % 5)
        };
        sql.push_str(&format!("INSERT INTO items VALUES({i},{g},'{text}');"));
    }
    sql.push_str("CREATE TABLE composite(k TEXT NOT NULL, n INTEGER NOT NULL, v INTEGER, PRIMARY KEY(k,n)); INSERT INTO composite VALUES('x',1,1),('x',2,NULL),('y',1,1); CREATE TABLE \"odd\"\"table\"(\"id\"\"key\" INTEGER PRIMARY KEY, \"v\"\"col\" TEXT); INSERT INTO \"odd\"\"table\" VALUES(1,'literal%_'),(2,'other');");
    sql
}
fn cmp(a: &Value, b: &Value, desc: bool) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        _ => {
            let c = match (a, b) {
                (Value::Int(a), Value::Int(b)) => a.cmp(b),
                (Value::Text(a), Value::Text(b)) => a.cmp(b),
                _ => panic!("reference fixture type"),
            };
            if desc {
                c.reverse()
            } else {
                c
            }
        }
    }
}
fn matches(v: &Value, p: &P) -> bool {
    match p {
        P::IsNull => *v == Value::Null,
        P::Contains(s) => matches!(v,Value::Text(t) if t.contains(s)),
        P::Prefix(s) => matches!(v,Value::Text(t) if t.starts_with(s)),
        _ if *v == Value::Null => false,
        P::Eq(x) => v == x,
        P::Lt(x) => cmp(v, x, false).is_lt(),
        P::Le(x) => !cmp(v, x, false).is_gt(),
        P::Gt(x) => cmp(v, x, false).is_gt(),
        P::Ge(x) => !cmp(v, x, false).is_lt(),
    }
}
async fn acceptance(source: &dyn Source) {
    let all = source.read_rows("items".into(), 1000).await.unwrap().rows;
    assert_eq!(all.len(), 47);
    let filters = vec![
        vec![],
        vec![TableFilter {
            column: "g".into(),
            predicate: P::IsNull,
        }],
        vec![
            TableFilter {
                column: "g".into(),
                predicate: P::Ge(Value::Int(1)),
            },
            TableFilter {
                column: "g".into(),
                predicate: P::Lt(Value::Int(3)),
            },
        ],
        vec![TableFilter {
            column: "label".into(),
            predicate: P::Contains("%_'; DROP TABLE items; --".into()),
        }],
        vec![TableFilter {
            column: "label".into(),
            predicate: P::Prefix("word".into()),
        }],
        vec![TableFilter {
            column: "g".into(),
            predicate: P::Eq(Value::Int(2)),
        }],
        vec![TableFilter {
            column: "g".into(),
            predicate: P::Le(Value::Int(0)),
        }],
        vec![TableFilter {
            column: "g".into(),
            predicate: P::Gt(Value::Int(2)),
        }],
    ];
    for descending in [false, true] {
        for filters in &filters {
            for sort in [
                vec![],
                vec![TableSort {
                    column: "g".into(),
                    descending,
                }],
                vec![
                    TableSort {
                        column: "g".into(),
                        descending,
                    },
                    TableSort {
                        column: "label".into(),
                        descending: !descending,
                    },
                ],
            ] {
                let mut expected: Vec<_> = all
                    .iter()
                    .filter(|r| {
                        filters
                            .iter()
                            .all(|f| matches(&r.values[&f.column], &f.predicate))
                    })
                    .cloned()
                    .collect();
                expected.sort_by(|a, b| {
                    sort.iter()
                        .map(|s| cmp(&a.values[&s.column], &b.values[&s.column], s.descending))
                        .find(|c| !c.is_eq())
                        .unwrap_or_else(|| cmp(&a.key["id"], &b.key["id"], false))
                });
                let mut query = TableQuery {
                    sort,
                    filters: filters.clone(),
                    limit: 3,
                    after: None,
                };
                let mut actual = vec![];
                loop {
                    let page = source
                        .query_rows("items".into(), query.clone())
                        .await
                        .unwrap();
                    actual.extend(page.rows);
                    assert!(actual.len() <= 47, "cursor loop");
                    let Some(next) = page.next else { break };
                    query.after = Some(next);
                }
                assert_eq!(actual, expected, "{query:?}");
            }
        }
    }
    let mut q = TableQuery {
        sort: vec![TableSort {
            column: "v".into(),
            descending: true,
        }],
        limit: 1,
        ..Default::default()
    };
    let mut keys = vec![];
    loop {
        let page = source
            .query_rows("composite".into(), q.clone())
            .await
            .unwrap();
        keys.extend(page.rows.into_iter().map(|r| r.key));
        let Some(next) = page.next else { break };
        q.after = Some(next);
        assert!(keys.len() < 4);
    }
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[0]["k"], Value::Text("x".into()));
    assert_eq!(keys[1]["k"], Value::Text("y".into()));
    assert_eq!(keys[2]["n"], Value::Int(2));
    let page = source
        .query_rows(
            "odd\"table".into(),
            TableQuery {
                sort: vec![TableSort {
                    column: "v\"col".into(),
                    descending: false,
                }],
                filters: vec![TableFilter {
                    column: "v\"col".into(),
                    predicate: P::Contains("%_".into()),
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].key["id\"key"], Value::Int(1));
    let evil = TableQuery {
        sort: vec![TableSort {
            column: "id\"; DROP TABLE items; --".into(),
            descending: false,
        }],
        ..Default::default()
    };
    assert!(source.query_rows("items".into(), evil).await.is_err());
    assert!(source
        .query_rows(
            "items\"; DROP TABLE items; --".into(),
            TableQuery::default()
        )
        .await
        .is_err());
    for q in [
        TableQuery {
            limit: 0,
            ..Default::default()
        },
        TableQuery {
            limit: 1001,
            ..Default::default()
        },
        TableQuery {
            after: Some(TableCursor {
                values: vec![Value::Int(1)],
                key: Default::default(),
            }),
            ..Default::default()
        },
    ] {
        assert!(source.query_rows("items".into(), q).await.is_err());
    }
    // Edit a row found past the first page and outside the old filter. Version
    // checks and exact-key conflict rereads must work there, too.
    let q = TableQuery {
        filters: vec![TableFilter {
            column: "id".into(),
            predicate: P::Eq(Value::Int(46)),
        }],
        ..Default::default()
    };
    let row = source
        .query_rows("items".into(), q)
        .await
        .unwrap()
        .rows
        .remove(0);
    let op = moonkale_core::RowOp {
        table: "items".into(),
        key: row.key,
        expected: Some(row.version),
        change: moonkale_core::RowChange::Update(std::collections::BTreeMap::from([(
            "g".into(),
            Value::Int(99),
        )])),
    };
    source
        .apply_rows(moonkale_core::RowBatch {
            ops: vec![op.clone()],
        })
        .await
        .unwrap();
    assert!(source
        .apply_rows(moonkale_core::RowBatch { ops: vec![op] })
        .await
        .is_err());
}
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_keyset() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("rows.sqlite");
    rusqlite::Connection::open(&p)
        .unwrap()
        .execute_batch(&seed())
        .unwrap();
    acceptance(&moonkale_sources_sql::sqlite::SqliteSource::open(&p).unwrap()).await;
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_keyset() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("rows.duckdb");
    duckdb::Connection::open(&p)
        .unwrap()
        .execute_batch(&seed())
        .unwrap();
    acceptance(&moonkale_sources_sql::duckdb::DuckDbSource::open(&p).unwrap()).await;
}
#[cfg(feature = "turso")]
#[tokio::test]
async fn turso_keyset() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("rows.turso");
    rusqlite::Connection::open(&p)
        .unwrap()
        .execute_batch(&seed())
        .unwrap();
    acceptance(
        &moonkale_sources_sql::turso::TursoSource::open(&p)
            .await
            .unwrap(),
    )
    .await;
}
