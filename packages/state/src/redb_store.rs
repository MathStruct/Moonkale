//! redb: the reference backend (Milestone 18 phase 3) — one file, pure Rust,
//! ACID, one writer at a time with readers on a snapshot (MVCC). A [`Batch`]
//! is one redb write transaction; each table is a `&[u8] → &[u8]` table.

use crate::{Batch, Entries, Op, StateError, StateStore};
use redb::{Database, ReadableDatabase, TableDefinition, TableError};
use std::path::Path;

pub struct RedbStore {
    db: Database,
}

fn err(e: impl std::fmt::Display) -> StateError {
    StateError::Storage(e.to_string())
}

fn def(name: &str) -> TableDefinition<'_, &'static [u8], &'static [u8]> {
    TableDefinition::new(name)
}

impl RedbStore {
    /// Open the database file at `path`, creating it if needed. redb has no
    /// relaxed mode that survives a killed process (`Durability::None`
    /// commits are lost unless a durable one follows), so every commit is
    /// durable.
    pub fn open(path: &Path) -> Result<Self, StateError> {
        Ok(Self {
            db: Database::create(path).map_err(err)?,
        })
    }
}

impl StateStore for RedbStore {
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        let txn = self.db.begin_read().map_err(err)?;
        let t = match txn.open_table(def(table)) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(err(e)),
        };
        Ok(t.get(key).map_err(err)?.map(|v| v.value().to_vec()))
    }

    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError> {
        let txn = self.db.begin_read().map_err(err)?;
        let t = match txn.open_table(def(table)) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(err(e)),
        };
        let mut out = Vec::new();
        for item in t.range(prefix..).map_err(err)? {
            let (k, v) = item.map_err(err)?;
            if !k.value().starts_with(prefix) {
                break;
            }
            out.push((k.value().to_vec(), v.value().to_vec()));
        }
        Ok(out)
    }

    fn write(&self, batch: Batch) -> Result<(), StateError> {
        let txn = self.db.begin_write().map_err(err)?;
        for op in &batch.ops {
            match op {
                Op::Put { table, key, value } => {
                    let mut t = txn.open_table(def(table)).map_err(err)?;
                    t.insert(key.as_slice(), value.as_slice()).map_err(err)?;
                }
                Op::Delete { table, key } => {
                    let mut t = txn.open_table(def(table)).map_err(err)?;
                    t.remove(key.as_slice()).map_err(err)?;
                }
            }
        }
        txn.commit().map_err(err)
    }
}
