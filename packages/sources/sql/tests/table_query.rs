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
    sql.push_str("CREATE TABLE many_cats(id INTEGER PRIMARY KEY, cat TEXT, val DOUBLE);");
    for i in 0..120 {
        sql.push_str(&format!(
            "INSERT INTO many_cats VALUES({i},'c{i:03}',{});",
            (i as f64) * 1.25 - 10.0
        ));
    }
    // Review regressions (2026-11-18): a column named like the bin alias, a
    // composite key whose second part decides first appearance, and finite
    // values whose range overflows f64.
    sql.push_str("CREATE TABLE collision(id INTEGER PRIMARY KEY, x DOUBLE, b INTEGER); INSERT INTO collision VALUES(1,1,0),(2,2,0),(3,3,0),(4,4,0),(5,5,0);");
    sql.push_str("CREATE TABLE cats2(k TEXT NOT NULL, n INTEGER NOT NULL, cat TEXT, val DOUBLE, PRIMARY KEY(k,n));");
    for n in 0..120 {
        sql.push_str(&format!(
            "INSERT INTO cats2 VALUES('same',{n},'c{:03}',1.0);",
            120 - n
        ));
    }
    sql.push_str("CREATE TABLE huge(id INTEGER PRIMARY KEY, x DOUBLE); INSERT INTO huge VALUES(1,-1e308),(2,0),(3,1e308);");
    sql.push_str("CREATE TABLE catsinf(id INTEGER PRIMARY KEY, cat TEXT, val DOUBLE); INSERT INTO catsinf VALUES(1,'big',1e308),(2,'big',1e308),(3,'small',1.0);");
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

    aggregate_acceptance(source).await;
}

fn brute_force_aggregate(
    rows: &[moonkale_core::RowSnapshot],
    agg: &moonkale_core::TableAggregate,
) -> moonkale_core::TableAggregateResult {
    use moonkale_core::{TableAggregateBucket, TableAggregateKind, TableAggregateResult};
    let num = |v: &Value| -> Option<f64> {
        match v {
            Value::Int(n) => Some(*n as f64),
            Value::Float(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    };
    let short = |v: &Value| -> String { v.to_string().chars().take(80).collect() };
    let total_rows = rows.len() as u64;
    match &agg.kind {
        TableAggregateKind::Histogram { log } => {
            let mut vals = Vec::new();
            let mut omitted = 0u64;
            for r in rows {
                let raw = r
                    .values
                    .get(&agg.x)
                    .or_else(|| r.key.get(&agg.x))
                    .and_then(num);
                let v = if *log {
                    raw.filter(|v| *v > 0.0).map(f64::log10)
                } else {
                    raw
                };
                match v {
                    Some(v) if v.is_finite() => vals.push(v),
                    _ => omitted += 1,
                }
            }
            if vals.is_empty() {
                return TableAggregateResult {
                    buckets: Vec::new(),
                    total_rows,
                    omitted: total_rows,
                    lo: None,
                    hi: None,
                };
            }
            let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let bins = if (hi - lo).abs() < f64::EPSILON {
                1
            } else {
                40
            };
            let width = if bins == 1 {
                1.0
            } else {
                (hi - lo) / bins as f64
            };
            let mut counts = vec![0u64; bins];
            for v in vals {
                let idx = if bins == 1 {
                    0
                } else {
                    (((v - lo) / width).floor() as isize).clamp(0, (bins - 1) as isize) as usize
                };
                counts[idx] += 1;
            }
            let buckets = counts
                .into_iter()
                .enumerate()
                .map(|(i, count)| TableAggregateBucket {
                    key: Value::Int(i as i64),
                    x_mid: lo + (i as f64 + 0.5) * width,
                    value: count as f64,
                    count,
                })
                .collect();
            TableAggregateResult {
                buckets,
                total_rows,
                omitted,
                lo: Some(lo),
                hi: Some(hi),
            }
        }
        TableAggregateKind::Bar { y } => {
            let mut groups: std::collections::BTreeMap<String, (Value, f64, u64)> =
                std::collections::BTreeMap::new();
            let mut omitted = 0u64;
            for r in rows {
                let Some(yv) = r.values.get(y).or_else(|| r.key.get(y)).and_then(num) else {
                    omitted += 1;
                    continue;
                };
                let xv = r
                    .values
                    .get(&agg.x)
                    .or_else(|| r.key.get(&agg.x))
                    .cloned()
                    .unwrap_or(Value::Null);
                let key = format!(
                    "{}\0{}",
                    short(&xv),
                    moonkale_core::NodeId::from_content(format!("{xv:?}").as_bytes())
                );
                if !groups.contains_key(&key)
                    && groups.len() >= moonkale_core::MAX_AGGREGATE_BUCKETS
                {
                    omitted += 1;
                    continue;
                }
                let entry = groups.entry(key).or_insert((xv, 0.0, 0));
                entry.1 += yv;
                entry.2 += 1;
            }
            let buckets = groups
                .into_values()
                .map(|(key, value, count)| TableAggregateBucket {
                    key,
                    x_mid: 0.0,
                    value,
                    count,
                })
                .collect();
            TableAggregateResult {
                buckets,
                total_rows,
                omitted,
                lo: None,
                hi: None,
            }
        }
    }
}

async fn aggregate_acceptance(source: &dyn Source) {
    use moonkale_core::{TableAggregate, TableAggregateKind};
    let all_rows = source.read_rows("items".into(), 1000).await.unwrap().rows;
    assert_eq!(all_rows.len(), 47);

    for agg in [
        TableAggregate {
            x: "g".into(),
            kind: TableAggregateKind::Histogram { log: false },
            filters: vec![],
        },
        TableAggregate {
            x: "g".into(),
            kind: TableAggregateKind::Histogram { log: true },
            filters: vec![],
        },
        TableAggregate {
            x: "g".into(),
            kind: TableAggregateKind::Histogram { log: false },
            filters: vec![TableFilter {
                column: "label".into(),
                predicate: P::Prefix("word".into()),
            }],
        },
        TableAggregate {
            x: "label".into(),
            kind: TableAggregateKind::Bar { y: "g".into() },
            filters: vec![],
        },
        TableAggregate {
            x: "label".into(),
            kind: TableAggregateKind::Bar { y: "g".into() },
            filters: vec![TableFilter {
                column: "id".into(),
                predicate: P::Le(Value::Int(20)),
            }],
        },
        // Non-numeric column histogram: all rows omitted.
        TableAggregate {
            x: "label".into(),
            kind: TableAggregateKind::Histogram { log: false },
            filters: vec![],
        },
    ] {
        let filtered_rows = if agg.filters.is_empty() {
            all_rows.clone()
        } else {
            source
                .query_rows(
                    "items".into(),
                    TableQuery {
                        filters: agg.filters.clone(),
                        limit: 1000,
                        ..Default::default()
                    },
                )
                .await
                .unwrap()
                .rows
        };
        let expected = brute_force_aggregate(&filtered_rows, &agg);
        let actual = source
            .aggregate_rows("items".into(), agg.clone())
            .await
            .unwrap();
        assert_eq!(actual.total_rows, expected.total_rows, "agg={agg:?}");
        assert_eq!(actual.omitted, expected.omitted, "agg={agg:?}");
        assert_eq!(actual.buckets.len(), expected.buckets.len(), "agg={agg:?}");
        for (a, b) in actual.buckets.iter().zip(expected.buckets.iter()) {
            assert_eq!(a.key, b.key, "agg={agg:?}");
            assert_eq!(a.count, b.count, "agg={agg:?}");
            assert!((a.x_mid - b.x_mid).abs() < 1e-9, "agg={agg:?}");
            assert!((a.value - b.value).abs() < 1e-9, "agg={agg:?}");
        }
    }

    // Over 100 distinct categories (`many_cats` has 120 distinct categories): checks the 100-category cap + omitted count + float histogram.
    let many_rows = source
        .read_rows("many_cats".into(), 1000)
        .await
        .unwrap()
        .rows;
    assert_eq!(many_rows.len(), 120);
    for agg in [
        TableAggregate {
            x: "cat".into(),
            kind: TableAggregateKind::Bar { y: "val".into() },
            filters: vec![],
        },
        TableAggregate {
            x: "val".into(),
            kind: TableAggregateKind::Histogram { log: false },
            filters: vec![],
        },
        TableAggregate {
            x: "val".into(),
            kind: TableAggregateKind::Histogram { log: true },
            filters: vec![],
        },
    ] {
        let expected = brute_force_aggregate(&many_rows, &agg);
        let actual = source
            .aggregate_rows("many_cats".into(), agg.clone())
            .await
            .unwrap();
        assert_eq!(actual.total_rows, expected.total_rows, "agg={agg:?}");
        assert_eq!(actual.omitted, expected.omitted, "agg={agg:?}");
        assert_eq!(actual.buckets.len(), expected.buckets.len(), "agg={agg:?}");
        for (a, b) in actual.buckets.iter().zip(expected.buckets.iter()) {
            assert_eq!(a.key, b.key, "agg={agg:?}");
            assert_eq!(a.count, b.count, "agg={agg:?}");
            assert!((a.x_mid - b.x_mid).abs() < 1e-9, "agg={agg:?}");
            assert!((a.value - b.value).abs() < 1e-9, "agg={agg:?}");
        }
    }

    // Bin alias vs. a source column named `b`; composite-key first appearance.
    for (table, agg) in [
        (
            "collision",
            TableAggregate {
                x: "x".into(),
                kind: TableAggregateKind::Histogram { log: false },
                filters: vec![],
            },
        ),
        (
            "collision",
            TableAggregate {
                x: "x".into(),
                kind: TableAggregateKind::Histogram { log: true },
                filters: vec![],
            },
        ),
        (
            "cats2",
            TableAggregate {
                x: "cat".into(),
                kind: TableAggregateKind::Bar { y: "val".into() },
                filters: vec![],
            },
        ),
    ] {
        let rows = source.read_rows(table.into(), 1000).await.unwrap().rows;
        let expected = brute_force_aggregate(&rows, &agg);
        let actual = source
            .aggregate_rows(table.into(), agg.clone())
            .await
            .unwrap_or_else(|e| panic!("{table} agg={agg:?}: {e}"));
        assert_eq!(
            actual.total_rows, expected.total_rows,
            "{table} agg={agg:?}"
        );
        assert_eq!(actual.omitted, expected.omitted, "{table} agg={agg:?}");
        let keys = |r: &moonkale_core::TableAggregateResult| {
            r.buckets
                .iter()
                .map(|b| (b.key.clone(), b.count))
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&actual), keys(&expected), "{table} agg={agg:?}");
    }

    // Finite values whose range overflows f64 are still binned, not omitted.
    let huge = source
        .aggregate_rows(
            "huge".into(),
            TableAggregate {
                x: "x".into(),
                kind: TableAggregateKind::Histogram { log: false },
                filters: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!((huge.total_rows, huge.omitted), (3, 0), "{huge:?}");
    let counted: Vec<(usize, u64)> = huge
        .buckets
        .iter()
        .enumerate()
        .filter(|(_, b)| b.count > 0)
        .map(|(i, b)| (i, b.count))
        .collect();
    assert_eq!(counted, vec![(0, 1), (20, 1), (39, 1)], "{huge:?}");
    assert!(huge.buckets.iter().all(|b| b.x_mid.is_finite()), "{huge:?}");

    // A category whose sum overflows f64 is omitted, not a failed aggregate.
    let inf = source
        .aggregate_rows(
            "catsinf".into(),
            TableAggregate {
                x: "cat".into(),
                kind: TableAggregateKind::Bar { y: "val".into() },
                filters: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!((inf.total_rows, inf.omitted), (3, 2), "{inf:?}");
    assert_eq!(inf.buckets.len(), 1, "{inf:?}");
    assert_eq!(inf.buckets[0].key, Value::Text("small".into()));
    assert_eq!(inf.buckets[0].value, 1.0);

    // Security & schema checks: unknown columns and tables are refused.
    assert!(source
        .aggregate_rows(
            "items".into(),
            TableAggregate {
                x: "g; DROP TABLE items".into(),
                kind: TableAggregateKind::Histogram { log: false },
                filters: vec![],
            },
        )
        .await
        .is_err());
    assert!(source
        .aggregate_rows(
            "missing_table".into(),
            TableAggregate {
                x: "g".into(),
                kind: TableAggregateKind::Histogram { log: false },
                filters: vec![],
            },
        )
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
