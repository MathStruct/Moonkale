//! Seeds one small store of each Milestone 17 kind into a folder, for the
//! E2E fixture (packages/web/tests/e2e/fixture.sh):
//!
//!   cargo run -p moonkale-sources-kv --features redb,rocksdb --example seed_stores -- <dir>
//!
//! `shop.turso` (users + a view), `app.redb` (typed tables), `events.rocksdb/`
//! (two column families), `people.helix/` (Person/City, KNOWS/LIVES_IN).
#![recursion_limit = "256"]

use std::path::Path;

#[tokio::main]
async fn main() {
    let dir = std::env::args().nth(1).expect("usage: seed_stores <dir>");
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();

    // Turso: SQLite's format, the Turso engine.
    let turso = dir.join("shop.turso");
    let _ = std::fs::remove_file(&turso);
    let db = turso::Builder::new_local(&turso.to_string_lossy())
        .build()
        .await
        .unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)",
        "INSERT INTO users VALUES (1, 'ada', 3.5), (2, 'alan', 2.0), (3, 'grace', 4.5)",
        "CREATE VIEW good AS SELECT name FROM users WHERE score > 3",
    ] {
        c.execute(sql, ()).await.unwrap();
    }
    drop(c);
    drop(db);

    // redb: typed tables.
    let redb_path = dir.join("app.redb");
    let _ = std::fs::remove_file(&redb_path);
    {
        use redb::{Database, TableDefinition};
        let db = Database::create(&redb_path).unwrap();
        let w = db.begin_write().unwrap();
        {
            let mut t = w
                .open_table(TableDefinition::<&str, &str>::new("settings"))
                .unwrap();
            t.insert("theme", "dark").unwrap();
            t.insert("font", "Inter").unwrap();
            let mut s = w
                .open_table(TableDefinition::<u64, f64>::new("scores"))
                .unwrap();
            s.insert(1, 3.5).unwrap();
            s.insert(2, 2.0).unwrap();
        }
        w.commit().unwrap();
    }

    // RocksDB: a directory with column families.
    let rocks = dir.join("events.rocksdb");
    let _ = std::fs::remove_dir_all(&rocks);
    {
        let mut o = rocksdb::Options::default();
        o.create_if_missing(true);
        o.create_missing_column_families(true);
        let db = rocksdb::DB::open_cf(&o, &rocks, ["default", "events"]).unwrap();
        let cf = db.cf_handle("events").unwrap();
        for (k, v) in [
            ("e:001", "login ada"),
            ("e:002", "save Home.md"),
            ("e:003", "logout ada"),
        ] {
            db.put_cf(cf, k, v).unwrap();
        }
        db.flush_cf(cf).unwrap();
    }

    // HelixDB, embedded: a root directory with the database `main`.
    let helix = dir.join("people.helix");
    let _ = std::fs::remove_dir_all(&helix);
    std::fs::create_dir_all(&helix).unwrap();
    {
        use helix_db::dsl::prelude::*;
        use helix_db::{Client, HelixDbSource, QueryRequest};
        let client = Client::open(HelixDbSource::Disk {
            root: helix.clone(),
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
    println!("seeded {}", dir.display());
}
