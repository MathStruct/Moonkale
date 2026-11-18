//! PLT-09: file-backed SQL sources report writes by other programs through
//! `changes_since`, and a query after the report sees the new rows.
use moonkale_core::Source;

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn a_data_folder_reports_an_appended_csv_and_reads_the_new_rows() {
    use moonkale_core::Query;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("metrics.csv");
    std::fs::write(&csv, "step,loss\n1,0.9\n2,0.8\n").unwrap();
    let s = moonkale_sources_sql::DuckDbSource::open_data_folder(dir.path()).unwrap();
    let base = s.changes_since(0).await.unwrap().expect("watched");
    assert!(!base.reset);
    let quiet = s.changes_since(base.seq).await.unwrap().unwrap();
    assert!(!quiet.reset, "reading does not count as a change");

    std::fs::OpenOptions::new()
        .append(true)
        .open(&csv)
        .unwrap()
        .write_all(b"3,0.7\n")
        .unwrap();
    let after = s.changes_since(base.seq).await.unwrap().unwrap();
    assert!(after.reset);
    let r = s
        .query(Query::Text {
            dialect: "sql".into(),
            text: "SELECT count(*) FROM metrics".into(),
        })
        .await
        .unwrap();
    let n = &r.table.unwrap().rows[0][0];
    assert_eq!(
        format!("{n:?}"),
        format!("{:?}", moonkale_core::Value::Int(3))
    );
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn a_sqlite_file_reports_a_write_by_another_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.sqlite");
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE m(step INTEGER, loss REAL);")
        .unwrap();
    let s = moonkale_sources_sql::SqliteSource::open(&path).unwrap();
    let base = s.changes_since(0).await.unwrap().expect("watched");
    c.execute("INSERT INTO m VALUES (1, 0.5)", []).unwrap();
    let after = s.changes_since(base.seq).await.unwrap().unwrap();
    assert!(after.reset, "a WAL write is seen");
    let quiet = s.changes_since(after.seq).await.unwrap().unwrap();
    assert!(!quiet.reset);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn a_duckdb_file_reports_a_write_by_another_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE m(step INTEGER, loss DOUBLE); INSERT INTO m VALUES (1, 0.5);")
        .unwrap();
    let s = moonkale_sources_sql::DuckDbSource::open(&path).unwrap();
    let base = s.changes_since(0).await.unwrap().expect("watched");
    assert!(!s.changes_since(base.seq).await.unwrap().unwrap().reset);
    // DuckDB's file handles last one operation, so another program can write.
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch("INSERT INTO m VALUES (2, 0.4); CHECKPOINT;")
        .unwrap();
    let after = s.changes_since(base.seq).await.unwrap().unwrap();
    assert!(after.reset, "a write to the file (or its .wal) is seen");
}

#[cfg(feature = "turso")]
#[tokio::test]
async fn a_turso_file_reports_a_write_by_another_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.turso");
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("CREATE TABLE m(step INTEGER, loss REAL); INSERT INTO m VALUES (1, 0.5);")
        .unwrap();
    let s = moonkale_sources_sql::TursoSource::open(&path)
        .await
        .unwrap();
    let base = s.changes_since(0).await.unwrap().expect("watched");
    assert!(!s.changes_since(base.seq).await.unwrap().unwrap().reset);
    c.execute("INSERT INTO m VALUES (2, 0.4)", []).unwrap();
    let after = s.changes_since(base.seq).await.unwrap().unwrap();
    assert!(after.reset, "a write to the file (or its -wal) is seen");
}
