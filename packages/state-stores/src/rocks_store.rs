//! RocksDB — an LSM key/value store (C++). One column family; the state
//! table is a key prefix (`Key::new().str(table)`), so a table scan is a
//! prefix scan. A batch is one `WriteBatch` (atomic).

use moonkale_state::{Batch, Durability, Entries, Key, Op, StateError, StateStore};
use rocksdb::{Direction, IteratorMode, Options, WriteBatch, WriteOptions, DB};
use std::path::Path;

pub struct RocksStore {
    db: DB,
    sync: bool,
}

fn err(e: impl std::fmt::Display) -> StateError {
    StateError::Storage(e.to_string())
}

fn full(table: &str, key: &[u8]) -> Vec<u8> {
    let mut k = Key::new().str(table).into_bytes();
    k.extend_from_slice(key);
    k
}

impl RocksStore {
    pub fn open(path: &Path) -> Result<Self, StateError> {
        Self::open_with(path, Durability::Durable)
    }

    /// `Durable` syncs the WAL on every write; `Relaxed` leaves it to the OS.
    pub fn open_with(path: &Path, durability: Durability) -> Result<Self, StateError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        Ok(Self {
            db: DB::open(&opts, path).map_err(err)?,
            sync: durability == Durability::Durable,
        })
    }
}

impl StateStore for RocksStore {
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        self.db.get(full(table, key)).map_err(err)
    }

    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError> {
        let head = Key::new().str(table).into_bytes();
        let start = full(table, prefix);
        let mut out = Vec::new();
        // One snapshot for the whole scan.
        let snap = self.db.snapshot();
        for item in snap.iterator(IteratorMode::From(&start, Direction::Forward)) {
            let (k, v) = item.map_err(err)?;
            if !k.starts_with(&start) {
                break;
            }
            out.push((k[head.len()..].to_vec(), v.to_vec()));
        }
        Ok(out)
    }

    fn write(&self, batch: Batch) -> Result<(), StateError> {
        let mut wb = WriteBatch::default();
        for op in &batch.ops {
            match op {
                Op::Put { table, key, value } => wb.put(full(table, key), value),
                Op::Delete { table, key } => wb.delete(full(table, key)),
            }
        }
        let mut wo = WriteOptions::default();
        wo.set_sync(self.sync);
        self.db.write_opt(wb, &wo).map_err(err)
    }
}
