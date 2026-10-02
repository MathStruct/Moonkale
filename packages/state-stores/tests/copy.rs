//! A migration between backends is a copy (ADR-0014).
use moonkale_state::{copy, tables, Batch, Key, MemoryStore, StateStore};

fn fill(s: &dyn StateStore) {
    let mut b = Batch::new();
    for seq in 0..2500u64 {
        b = b.put(
            tables::EVENTS,
            Key::new().str("folder:a").u64(seq),
            seq.to_string(),
        );
    }
    b = b
        .put(
            tables::SETTINGS,
            Key::new().str("user"),
            br#"{"v":1,"data":{}}"#.to_vec(),
        )
        .put(
            tables::LAYOUT,
            Key::new().str("folder:a"),
            b"layout".to_vec(),
        );
    s.write(b).unwrap();
}

fn same(a: &dyn StateStore, b: &dyn StateStore) {
    for t in tables::ALL {
        assert_eq!(
            a.scan(t, b"").unwrap(),
            b.scan(t, b"").unwrap(),
            "table {t}"
        );
    }
}

#[test]
fn memory_to_memory() {
    let (a, b) = (MemoryStore::new(), MemoryStore::new());
    fill(&a);
    assert_eq!(copy(&a, &b, tables::ALL).unwrap(), 2502);
    same(&a, &b);
}

#[cfg(all(feature = "sqlite", feature = "redb"))]
#[test]
fn sqlite_to_redb_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let sqlite =
        moonkale_state_stores::SqliteStore::open(&dir.path().join("state.sqlite")).unwrap();
    let redb = moonkale_state_stores::RedbStore::open(&dir.path().join("state.redb")).unwrap();
    fill(&sqlite);
    copy(&sqlite, &redb, tables::ALL).unwrap();
    same(&sqlite, &redb);
    let back = MemoryStore::new();
    copy(&redb, &back, tables::ALL).unwrap();
    same(&sqlite, &back);
}
