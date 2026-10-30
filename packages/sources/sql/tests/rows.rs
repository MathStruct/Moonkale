//! All fixtures are new temporary databases, never user files.
use moonkale_core::{RowBatch, RowChange, RowOp, Source, SourceError, Value};
use std::collections::BTreeMap;
fn values(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}
async fn acceptance(source: &dyn Source) {
    let first = source
        .read_rows("items".into(), 100)
        .await
        .unwrap()
        .rows
        .remove(0);
    let update = RowOp {
        table: "items".into(),
        key: first.key.clone(),
        expected: Some(first.version),
        change: RowChange::Update(values(&[(
            "name",
            Value::Text("Robert'); DROP TABLE items; --".into()),
        )])),
    };
    // Preview reserves nothing and changes nothing.
    source
        .preview_rows(RowBatch {
            ops: vec![update.clone()],
        })
        .await
        .unwrap();
    assert_eq!(
        source.read_rows("items".into(), 100).await.unwrap().rows[0],
        first
    );
    let updated = source
        .apply_rows(RowBatch {
            ops: vec![update.clone()],
        })
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_ne!(updated.version, first.version);
    assert_eq!(
        updated.values["name"],
        Value::Text("Robert'); DROP TABLE items; --".into())
    );
    assert!(matches!(
        source.apply_rows(RowBatch { ops: vec![update] }).await,
        Err(SourceError::Conflict { .. })
    ));
    let change = RowOp {
        table: "items".into(),
        key: updated.key.clone(),
        expected: Some(updated.version),
        change: RowChange::Update(values(&[("name", Value::Text("rollback".into()))])),
    };
    let mut invalid = change.clone();
    invalid.key = values(&[("id", Value::Int(999))]);
    invalid.change = RowChange::Update(values(&[("name\"; DROP TABLE items; --", Value::Int(1))]));
    assert!(source
        .apply_rows(RowBatch {
            ops: vec![change, invalid]
        })
        .await
        .is_err());
    assert_eq!(
        source.read_rows("items".into(), 100).await.unwrap().rows[0],
        updated
    );
    let first_write = RowOp {
        table: "items".into(),
        key: updated.key.clone(),
        expected: Some(updated.version),
        change: RowChange::Update(values(&[("name", Value::Text("must roll back".into()))])),
    };
    let bad_insert = RowOp {
        table: "checked".into(),
        key: values(&[("id", Value::Int(1))]),
        expected: None,
        change: RowChange::Insert(values(&[("id", Value::Int(1)), ("n", Value::Int(-1))])),
    };
    assert!(source
        .apply_rows(RowBatch {
            ops: vec![first_write, bad_insert]
        })
        .await
        .is_err());
    assert_eq!(
        source.read_rows("items".into(), 100).await.unwrap().rows[0],
        updated
    );

    let insert = RowOp {
        table: "items".into(),
        key: values(&[("id", Value::Int(2))]),
        expected: None,
        change: RowChange::Insert(values(&[
            ("id", Value::Int(2)),
            ("name", Value::Text("two".into())),
        ])),
    };
    let inserted = source
        .apply_rows(RowBatch {
            ops: vec![insert.clone()],
        })
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert!(matches!(
        source.apply_rows(RowBatch { ops: vec![insert] }).await,
        Err(SourceError::Conflict { .. })
    ));
    let delete = RowOp {
        table: "items".into(),
        key: inserted.key,
        expected: Some(inserted.version),
        change: RowChange::Delete,
    };
    assert!(source
        .apply_rows(RowBatch { ops: vec![delete] })
        .await
        .unwrap()[0]
        .is_none());
    assert_eq!(
        source
            .read_rows("items".into(), 100)
            .await
            .unwrap()
            .rows
            .len(),
        1
    );
    for table in ["v", "generated"] {
        let page = source.read_rows(table.into(), 100).await.unwrap();
        assert!(page.refusal.is_some(), "{table}");
        let op = RowOp {
            table: table.into(),
            key: first.key.clone(),
            expected: Some(first.version),
            change: RowChange::Delete,
        };
        assert!(source.apply_rows(RowBatch { ops: vec![op] }).await.is_err());
    }
    assert!(source
        .read_rows("items\"; DROP TABLE items; --".into(), 100)
        .await
        .is_err());
    // A malicious identifier that is not in the catalog cannot become SQL.
    let bad_column = RowOp {
        table: "items".into(),
        key: updated.key.clone(),
        expected: Some(updated.version),
        change: RowChange::Update(values(&[("name\"; DROP TABLE items; --", Value::Int(1))])),
    };
    assert!(source
        .apply_rows(RowBatch {
            ops: vec![bad_column]
        })
        .await
        .is_err());
    let mut wrong_key = updated.key.clone();
    wrong_key.insert("id OR 1=1".into(), Value::Int(1));
    assert!(source
        .apply_rows(RowBatch {
            ops: vec![RowOp {
                table: "items".into(),
                key: wrong_key,
                expected: Some(updated.version),
                change: RowChange::Delete
            }]
        })
        .await
        .is_err());
    let weird = "odd\"; DROP TABLE items; --";
    let insert = RowOp {
        table: weird.into(),
        key: values(&[("id", Value::Int(1))]),
        expected: None,
        change: RowChange::Insert(values(&[
            ("id", Value::Int(1)),
            ("name", Value::Text("safe".into())),
        ])),
    };
    source
        .apply_rows(RowBatch { ops: vec![insert] })
        .await
        .unwrap();
    assert_eq!(
        source.read_rows(weird.into(), 10).await.unwrap().rows[0].values["name"],
        Value::Text("safe".into())
    );
    assert_eq!(
        source.read_rows("items".into(), 10).await.unwrap().rows[0],
        updated
    );
    let typed = source
        .read_rows("typed".into(), 10)
        .await
        .unwrap()
        .rows
        .remove(0);
    assert!(matches!(typed.values["instant"], Value::DateTime(_)));
    let next = Value::DateTime(moonkale_core::DateTimeCell::parse("2026-10-30 11:45:00").unwrap());
    let changed = source
        .apply_rows(RowBatch {
            ops: vec![RowOp {
                table: "typed".into(),
                key: typed.key,
                expected: Some(typed.version),
                change: RowChange::Update(values(&[("instant", next.clone())])),
            }],
        })
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(changed.values["instant"], next);
    if !source.id().as_str().starts_with("duckdb:") {
        let raw = changed.stored["instant"].clone();
        let equivalent = source
            .apply_rows(RowBatch {
                ops: vec![RowOp {
                    table: "typed".into(),
                    key: changed.key.clone(),
                    expected: Some(changed.version),
                    change: RowChange::Update(values(&[(
                        "instant",
                        Value::Text("2026-10-30 11:45:00.000".into()),
                    )])),
                }],
            })
            .await
            .unwrap()
            .remove(0)
            .unwrap();
        assert_eq!(equivalent.values, changed.values);
        assert_ne!(
            equivalent.version, changed.version,
            "native formatting changes are writes too"
        );
        let undo = source
            .apply_rows(RowBatch {
                ops: vec![RowOp {
                    table: "typed".into(),
                    key: equivalent.key,
                    expected: Some(equivalent.version),
                    change: RowChange::Update(values(&[("instant", raw)])),
                }],
            })
            .await
            .unwrap()
            .remove(0)
            .unwrap();
        assert_eq!(
            undo.version, changed.version,
            "undo restores the exact native value"
        );
    }

    let dated = source
        .read_rows("dated_key".into(), 10)
        .await
        .unwrap()
        .rows
        .remove(0);
    assert!(matches!(dated.values["d"], Value::DateTime(_)));
    let changed = source
        .apply_rows(RowBatch {
            ops: vec![RowOp {
                table: "dated_key".into(),
                key: dated.key,
                expected: Some(dated.version),
                change: RowChange::Update(values(&[("n", Value::Int(2))])),
            }],
        })
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(changed.values["n"], Value::Int(2));
    assert!(source.descriptor().capabilities.atomic_rows);
}
const SEED: &str = r#"CREATE TABLE items(id INTEGER PRIMARY KEY, name TEXT); INSERT INTO items VALUES(1,'one'); CREATE VIEW v AS SELECT * FROM items; CREATE TABLE generated(id INTEGER PRIMARY KEY, n INTEGER GENERATED ALWAYS AS (id + 1)); CREATE TABLE dated_key(d DATE PRIMARY KEY NOT NULL, n INTEGER); INSERT INTO dated_key VALUES('2026-10-23',1); CREATE TABLE checked(id INTEGER PRIMARY KEY, n INTEGER CHECK (n > 0)); CREATE TABLE typed(id INTEGER PRIMARY KEY, instant DATETIME); INSERT INTO typed VALUES(1,'2026-10-23 10:30:00'); CREATE TABLE keyless(name TEXT); CREATE TABLE shadow(rowid TEXT, _rowid_ TEXT, oid TEXT); CREATE TABLE "odd""; DROP TABLE items; --"(id INTEGER PRIMARY KEY, name TEXT);"#;
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.sqlite");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(SEED)
        .unwrap();
    let source = moonkale_sources_sql::sqlite::SqliteSource::open(&path).unwrap();
    acceptance(&source).await;
    assert!(source
        .read_rows("shadow".into(), 10)
        .await
        .unwrap()
        .refusal
        .is_some());
    assert!(source
        .read_rows("keyless".into(), 10)
        .await
        .unwrap()
        .refusal
        .is_none());
    let rowid = RowOp {
        table: "keyless".into(),
        key: values(&[("_rowid_", Value::Int(17))]),
        expected: None,
        change: RowChange::Insert(values(&[
            ("_rowid_", Value::Int(17)),
            ("name", Value::Text("rowid".into())),
        ])),
    };
    let inserted = source
        .apply_rows(RowBatch { ops: vec![rowid] })
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(inserted.key["_rowid_"], Value::Int(17));
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch(SEED)
        .unwrap();
    let source = moonkale_sources_sql::duckdb::DuckDbSource::open(&path).unwrap();
    acceptance(&source).await;
    duckdb::Connection::open(&path).unwrap().execute_batch("CREATE TABLE vectors(id INTEGER PRIMARY KEY, samples DOUBLE[]); INSERT INTO vectors VALUES(1,[1.0,2.0])").unwrap();
    let vector = source
        .read_rows("vectors".into(), 10)
        .await
        .unwrap()
        .rows
        .remove(0);
    let samples = Value::Vector(moonkale_core::NumericVector::new(vec![2., 4., 8.]).unwrap());
    let result = source
        .apply_rows(RowBatch {
            ops: vec![RowOp {
                table: "vectors".into(),
                key: vector.key,
                expected: Some(vector.version),
                change: RowChange::Update(values(&[("samples", samples.clone())])),
            }],
        })
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(result.values["samples"], samples);
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE SEQUENCE guarded_sequence START 1")
        .unwrap();
    assert!(source
        .query(moonkale_core::Query::Text {
            dialect: "sql".into(),
            text: "SELECT nextval('guarded_sequence')".into()
        })
        .await
        .is_err());
    let next: i64 = duckdb::Connection::open(&path)
        .unwrap()
        .query_row("SELECT nextval('guarded_sequence')", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        next, 1,
        "a SELECT cannot mutate a sequence through raw query text"
    );
    duckdb::Connection::open(&path).unwrap().execute_batch("CREATE SEQUENCE preview_sequence START 1; CREATE TABLE defaults_test(id INTEGER PRIMARY KEY, n BIGINT DEFAULT nextval('preview_sequence'))").unwrap();
    source
        .preview_rows(RowBatch {
            ops: vec![RowOp {
                table: "defaults_test".into(),
                key: values(&[("id", Value::Int(7))]),
                expected: None,
                change: RowChange::Insert(values(&[("id", Value::Int(7))])),
            }],
        })
        .await
        .unwrap();
    let untouched: i64 = duckdb::Connection::open(&path)
        .unwrap()
        .query_row("SELECT nextval('preview_sequence')", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        untouched, 1,
        "review preview must never execute sequence defaults"
    );

    assert!(source
        .read_rows("keyless".into(), 10)
        .await
        .unwrap()
        .refusal
        .is_some());
}
#[cfg(feature = "turso")]
#[tokio::test]
async fn turso_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.turso");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(SEED)
        .unwrap();
    let source = moonkale_sources_sql::turso::TursoSource::open(&path)
        .await
        .unwrap();
    acceptance(&source).await;
}

#[cfg(unix)]
#[tokio::test]
async fn non_utf8_database_paths_cannot_alias_another_source_or_create_a_lossy_path() {
    use std::os::unix::ffi::OsStringExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join(std::ffi::OsString::from_vec(b"db-\xff.sqlite".to_vec()));
    std::fs::File::create(&path).unwrap();
    #[cfg(feature = "sqlite")]
    assert!(matches!(
        moonkale_sources_sql::sqlite::SqliteSource::open(&path),
        Err(SourceError::Invalid(_))
    ));
    #[cfg(feature = "duckdb")]
    assert!(matches!(
        moonkale_sources_sql::duckdb::DuckDbSource::open(&path),
        Err(SourceError::Invalid(_))
    ));
    #[cfg(feature = "turso")]
    assert!(matches!(
        moonkale_sources_sql::turso::TursoSource::open(&path).await,
        Err(SourceError::Invalid(_))
    ));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
