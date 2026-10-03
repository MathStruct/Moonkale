//! Milestone 17: redb and RocksDB as sources — tables in the tree, the `kv`
//! dialect, typed redb tables, a RocksDB database another writer keeps open.

use moonkale_core::{NodeKind, Query, Source, Value};

fn text(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        other => format!("{other:?}"),
    }
}

async fn kv(src: &dyn Source, q: &str) -> Vec<Vec<String>> {
    let r = src
        .query(Query::Text {
            dialect: "kv".into(),
            text: q.into(),
        })
        .await
        .unwrap_or_else(|e| panic!("{q}: {e}"));
    r.table
        .unwrap()
        .rows
        .iter()
        .map(|row| row.iter().map(text).collect())
        .collect()
}

#[cfg(feature = "redb")]
#[tokio::test]
async fn redb_tables_types_and_scans() {
    use redb::{Database, TableDefinition};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.redb");
    {
        let db = Database::create(&path).unwrap();
        let w = db.begin_write().unwrap();
        {
            let mut users = w
                .open_table(TableDefinition::<&str, &str>::new("users"))
                .unwrap();
            for (k, v) in [("ada", "Lovelace"), ("alan", "Turing"), ("grace", "Hopper")] {
                users.insert(k, v).unwrap();
            }
            let mut scores = w
                .open_table(TableDefinition::<u64, f64>::new("scores"))
                .unwrap();
            scores.insert(7, 0.5).unwrap();
            let mut raw = w
                .open_table(TableDefinition::<&[u8], &[u8]>::new("raw"))
                .unwrap();
            raw.insert(&[0u8, 1][..], &[0xffu8][..]).unwrap();
            // A table of a type this build does not know: listed, not read.
            let mut odd = w
                .open_table(TableDefinition::<(u32, u32), u8>::new("odd"))
                .unwrap();
            odd.insert((1, 2), 3).unwrap();
        }
        w.commit().unwrap();
    }
    let src = moonkale_sources_kv::open_redb(&path).unwrap();
    let d = src.descriptor();
    assert_eq!(d.display_name, "app.redb");

    let tables = src.query(Query::Children(d.root)).await.unwrap();
    let labels: Vec<_> = tables.nodes.iter().map(|n| n.label.clone()).collect();
    assert!(tables.nodes.iter().all(|n| n.kind == NodeKind::Table));
    assert!(labels.iter().any(|l| l == "users · 3"), "{labels:?}");
    assert!(
        labels
            .iter()
            .any(|l| l.starts_with("odd · 1 (") && l.ends_with("not readable)")),
        "{labels:?}"
    );

    assert_eq!(
        kv(&src, "scan users prefix a").await,
        [["ada", "Lovelace"], ["alan", "Turing"]]
    );
    assert_eq!(kv(&src, "get users grace").await, [["grace", "Hopper"]]);
    assert_eq!(kv(&src, "scan scores").await, [["7", "0.5"]]);
    assert_eq!(kv(&src, "scan raw").await, [["0x0001", "0xff"]]);
    assert_eq!(kv(&src, "scan users limit 1").await.len(), 1);
    let types = kv(&src, "tables").await;
    assert!(types.contains(&vec![
        "scores".to_string(),
        "u64".into(),
        "f64".into(),
        "1".into()
    ]));
    assert!(src
        .query(Query::Text {
            dialect: "kv".into(),
            text: "scan odd".into()
        })
        .await
        .is_err());
}

#[cfg(feature = "rocksdb")]
#[tokio::test]
async fn rocksdb_families_and_a_writer_alongside() {
    use rocksdb::{Options, DB};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.rocksdb");
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    // The writer stays open for the whole test.
    let writer = DB::open_cf(&opts, &path, ["default", "users"]).unwrap();
    let users = writer.cf_handle("users").unwrap();
    writer.put_cf(users, b"u:1", b"ada").unwrap();
    writer.put_cf(users, b"u:2", b"alan").unwrap();
    writer.put_cf(users, b"v:1", [0u8, 159]).unwrap();
    writer.flush_cf(users).unwrap();

    let src = moonkale_sources_kv::open_rocksdb(&path).unwrap();
    let tables = src
        .query(Query::Children(src.descriptor().root))
        .await
        .unwrap();
    let names: Vec<_> = tables.nodes.iter().map(|n| n.native_key.clone()).collect();
    assert_eq!(names, ["table:default", "table:users"]);

    assert_eq!(
        kv(&src, "scan users prefix u:").await,
        [["u:1", "ada"], ["u:2", "alan"]]
    );
    assert_eq!(kv(&src, "get users v:1").await, [["v:1", "0x009f"]]);

    // Written after Moonkale opened it: the secondary catches up.
    writer.put_cf(users, b"u:3", b"grace").unwrap();
    writer.flush_cf(users).unwrap();
    assert_eq!(kv(&src, "scan users prefix u:3").await, [["u:3", "grace"]]);
}
