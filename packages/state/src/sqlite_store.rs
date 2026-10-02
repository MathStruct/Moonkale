//! SQLite (`rusqlite`, bundled) — the baseline of the comparison: one file,
//! WAL, a batch is one transaction. The connection sits behind a mutex
//! (rusqlite connections are `Send`, not `Sync`).

use crate::sql::{prefix_end, SCHEMA};
use crate::{Batch, Durability, Entries, Op, StateError, StateStore};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

fn err(e: impl std::fmt::Display) -> StateError {
    StateError::Storage(e.to_string())
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, StateError> {
        Self::open_with(path, Durability::Durable)
    }

    /// WAL either way; `Durable` is `synchronous=FULL` (fsync per commit),
    /// `Relaxed` is `synchronous=NORMAL` (fsync at checkpoints).
    pub fn open_with(path: &Path, durability: Durability) -> Result<Self, StateError> {
        let conn = Connection::open(path).map_err(err)?;
        let sync = match durability {
            Durability::Durable => "FULL",
            Durability::Relaxed => "NORMAL",
        };
        conn.execute_batch(&format!(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous={sync};"
        ))
        .map_err(err)?;
        conn.execute_batch(SCHEMA).map_err(err)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl StateStore for SqliteStore {
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT v FROM kv WHERE t = ?1 AND k = ?2",
            params![table, key],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)
    }

    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError> {
        let c = self.conn.lock().unwrap();
        let map = |r: &rusqlite::Row| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?));
        let rows: Result<Entries, _> = match prefix_end(prefix) {
            Some(end) => c
                .prepare_cached(
                    "SELECT k, v FROM kv WHERE t = ?1 AND k >= ?2 AND k < ?3 ORDER BY k",
                )
                .map_err(err)?
                .query_map(params![table, prefix, end], map)
                .map_err(err)?
                .collect(),
            None => c
                .prepare_cached("SELECT k, v FROM kv WHERE t = ?1 AND k >= ?2 ORDER BY k")
                .map_err(err)?
                .query_map(params![table, prefix], map)
                .map_err(err)?
                .collect(),
        };
        rows.map_err(err)
    }

    fn write(&self, batch: Batch) -> Result<(), StateError> {
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction().map_err(err)?;
        for op in &batch.ops {
            match op {
                Op::Put { table, key, value } => tx
                    .execute(
                        "INSERT INTO kv (t, k, v) VALUES (?1, ?2, ?3) ON CONFLICT (t, k) DO UPDATE SET v = excluded.v",
                        params![table, key, value],
                    )
                    .map_err(err)?,
                Op::Delete { table, key } => tx
                    .execute("DELETE FROM kv WHERE t = ?1 AND k = ?2", params![table, key])
                    .map_err(err)?,
            };
        }
        tx.commit().map_err(err)
    }
}
