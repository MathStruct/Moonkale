//! Every backend through the conformance suite.
use moonkale_state::testing::conformance;
use moonkale_state::{MemoryStore, StateStore};
use std::sync::Arc;

#[test]
fn memory() {
    conformance(|| Arc::new(MemoryStore::new()) as Arc<dyn StateStore>);
}

#[cfg(feature = "redb")]
#[test]
fn redb() {
    let dir = tempfile::tempdir().unwrap();
    let n = std::sync::atomic::AtomicUsize::new(0);
    conformance(|| {
        let i = n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Arc::new(
            moonkale_state_stores::RedbStore::open(&dir.path().join(format!("s{i}.redb"))).unwrap(),
        ) as Arc<dyn StateStore>
    });
    moonkale_state::testing::persistence(|p| {
        Arc::new(moonkale_state_stores::RedbStore::open(p).unwrap()) as Arc<dyn StateStore>
    });
}

/// A file-backed backend through both suites: `open(path)` per store.
#[allow(dead_code)]
fn file_backend(open: fn(&std::path::Path) -> Arc<dyn StateStore>, ext: &str) {
    let dir = tempfile::tempdir().unwrap();
    let n = std::sync::atomic::AtomicUsize::new(0);
    conformance(|| {
        let i = n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        open(&dir.path().join(format!("s{i}.{ext}")))
    });
    moonkale_state::testing::persistence(open);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite() {
    file_backend(
        |p| Arc::new(moonkale_state_stores::SqliteStore::open(p).unwrap()),
        "sqlite",
    );
}

#[cfg(feature = "turso")]
#[test]
fn turso() {
    file_backend(
        |p| Arc::new(moonkale_state_stores::TursoStore::open(p).unwrap()),
        "turso",
    );
}

#[cfg(feature = "rocksdb")]
#[test]
fn rocksdb() {
    file_backend(
        |p| Arc::new(moonkale_state_stores::RocksStore::open(p).unwrap()),
        "rocksdb",
    );
}

#[cfg(feature = "helix")]
#[test]
fn helix() {
    file_backend(
        |p| Arc::new(moonkale_state_stores::HelixStore::open(p).unwrap()),
        "helix",
    );
}

/// The suite is not vacuous: a store that forgets deletes fails it.
#[test]
#[should_panic]
fn a_broken_backend_fails_the_suite() {
    use moonkale_state::{Batch, Entries, Op, StateError};
    struct ForgetsDeletes(MemoryStore);
    impl StateStore for ForgetsDeletes {
        fn get(&self, t: &str, k: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
            self.0.get(t, k)
        }
        fn scan(&self, t: &str, p: &[u8]) -> Result<Entries, StateError> {
            self.0.scan(t, p)
        }
        fn write(&self, b: Batch) -> Result<(), StateError> {
            let ops = b
                .ops
                .into_iter()
                .filter(|o| matches!(o, Op::Put { .. }))
                .collect();
            self.0.write(Batch { ops })
        }
    }
    conformance(|| Arc::new(ForgetsDeletes(MemoryStore::new())) as Arc<dyn StateStore>);
}
