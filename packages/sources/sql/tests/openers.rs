//! The openers the desktop and the server use (Milestone 18 phase 2): a
//! path is claimed by name and opened by the right driver.
#![cfg(all(feature = "sqlite", feature = "duckdb"))]
use moonkale_core::{Openers, SourceFamily};

#[tokio::test]
async fn sqlite_files_and_data_folders_open_through_the_openers() {
    let dir = tempfile::tempdir().unwrap();
    rusqlite::Connection::open(dir.path().join("t.sqlite"))
        .unwrap()
        .execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY);")
        .unwrap();
    std::fs::write(dir.path().join("people.csv"), "name,age\nAda,36\n").unwrap();
    let openers = Openers::new(moonkale_sources_sql::openers());

    assert!(openers.is_database("notes/t.sqlite", false));
    assert!(
        !openers.is_database("notes/t.sqlite", true),
        "a directory is not a SQLite file"
    );
    assert!(!openers.is_database("README.md", false));

    let db = openers
        .open(dir.path().join("t.sqlite"), false)
        .await
        .unwrap()
        .expect("claimed");
    assert_eq!(db.descriptor().family, SourceFamily::Sql);
    assert!(db.id().as_str().contains("sqlite"), "{}", db.id().as_str());

    // A data file opens its folder as DuckDB views.
    let data = openers
        .open(dir.path().join("people.csv"), false)
        .await
        .unwrap()
        .expect("claimed");
    assert!(
        data.id().as_str().contains("duckdb"),
        "{}",
        data.id().as_str()
    );

    assert!(openers
        .open(dir.path().join("README.md"), false)
        .await
        .unwrap()
        .is_none());
}
