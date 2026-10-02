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
        Arc::new(moonkale_state::RedbStore::open(&dir.path().join(format!("s{i}.redb"))).unwrap())
            as Arc<dyn StateStore>
    });
    moonkale_state::testing::persistence(|p| {
        Arc::new(moonkale_state::RedbStore::open(p).unwrap()) as Arc<dyn StateStore>
    });
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
