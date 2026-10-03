//! An in-memory store: the semantics of the trait in their simplest form,
//! for tests and for a window without persistence.

use crate::{Batch, Entries, Op, StateError, StateStore};
use std::collections::BTreeMap;
use std::sync::RwLock;

type Tables = BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>;

#[derive(Default)]
pub struct MemoryStore {
    tables: RwLock<Tables>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StateStore for MemoryStore {
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        let t = self.tables.read().unwrap();
        Ok(t.get(table).and_then(|t| t.get(key)).cloned())
    }

    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError> {
        let t = self.tables.read().unwrap();
        Ok(t.get(table)
            .map(|t| {
                t.range(prefix.to_vec()..)
                    .take_while(|(k, _)| k.starts_with(prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn write(&self, batch: Batch) -> Result<(), StateError> {
        // One lock for the whole batch: readers see it all or not at all.
        let mut t = self.tables.write().unwrap();
        for op in batch.ops {
            match op {
                Op::Put { table, key, value } => {
                    t.entry(table).or_default().insert(key, value);
                }
                Op::Delete { table, key } => {
                    if let Some(t) = t.get_mut(&table) {
                        t.remove(&key);
                    }
                }
            }
        }
        Ok(())
    }
}
