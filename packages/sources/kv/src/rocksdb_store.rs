//! RocksDB (Milestone 17): an LSM key/value store (C++, through the
//! `rocksdb` crate); a database is a directory, its tables are column
//! families of byte keys and values.
//!
//! Opened as a **secondary instance**: RocksDB allows one writer per
//! directory, and a secondary reads alongside it without taking the lock —
//! so a database an application is writing stays openable here.
//! `try_catch_up_with_primary` runs before every read, so new writes show up.
//! The secondary keeps its own small directory of metadata under the
//! system's temp dir.

use crate::kv::{show_bytes, KvStore, KvTable, Rows};
use rocksdb::{Direction, IteratorMode, Options, DB};
use std::path::Path;

pub struct RocksStore {
    db: DB,
    families: Vec<String>,
}

impl RocksStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let opts = Options::default();
        let families = DB::list_cf(&opts, path).map_err(|e| e.to_string())?;
        // One secondary directory per primary path, reused across opens.
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&path, &mut h);
        let secondary = std::env::temp_dir().join(format!(
            "moonkale-rocksdb-secondary-{:016x}",
            std::hash::Hasher::finish(&h)
        ));
        std::fs::create_dir_all(&secondary).map_err(|e| e.to_string())?;
        let mut opts = Options::default();
        // A secondary must keep every file open it may read (RocksDB docs).
        opts.set_max_open_files(-1);
        let db = DB::open_cf_as_secondary(&opts, path, secondary.as_path(), &families)
            .map_err(|e| e.to_string())?;
        Ok(Self { db, families })
    }

    fn catch_up(&self) {
        let _ = self.db.try_catch_up_with_primary();
    }

    fn family(&self, name: &str) -> Result<&rocksdb::ColumnFamily, String> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| format!("no column family {name}"))
    }
}

impl KvStore for RocksStore {
    fn tables(&self) -> Result<Vec<KvTable>, String> {
        self.catch_up();
        let mut out = Vec::new();
        for name in &self.families {
            let len = self
                .family(name)
                .ok()
                .and_then(|cf| {
                    self.db
                        .property_int_value_cf(cf, "rocksdb.estimate-num-keys")
                        .ok()
                })
                .flatten();
            out.push(KvTable {
                name: name.clone(),
                key_type: "bytes".into(),
                value_type: "bytes".into(),
                len,
                readable: true,
            });
        }
        Ok(out)
    }

    fn scan(&self, table: &str, prefix: &[u8], limit: usize) -> Result<Rows, String> {
        self.catch_up();
        let cf = self.family(table)?;
        let mut rows = Vec::new();
        for item in self
            .db
            .iterator_cf(cf, IteratorMode::From(prefix, Direction::Forward))
        {
            let (k, v) = item.map_err(|e| e.to_string())?;
            if !k.starts_with(prefix) {
                break;
            }
            if rows.len() == limit {
                return Ok((rows, true));
            }
            rows.push((show_bytes(&k), show_bytes(&v)));
        }
        Ok((rows, false))
    }

    fn get(&self, table: &str, key: &[u8]) -> Result<Option<String>, String> {
        self.catch_up();
        let cf = self.family(table)?;
        Ok(self
            .db
            .get_cf(cf, key)
            .map_err(|e| e.to_string())?
            .map(|v| show_bytes(&v)))
    }
}
