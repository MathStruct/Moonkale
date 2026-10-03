//! Turso (the in-process Rust rewrite of SQLite, beta) — the same schema as
//! [`crate::SqliteStore`]. Turso's API is async; this store owns a small
//! Tokio runtime and blocks on it, so it must not be called from inside an
//! async task (an app would wrap it in `spawn_blocking`). The connection
//! sits behind a mutex.

use crate::sql::{prefix_end, SCHEMA_ROWID};
use moonkale_state::{Batch, Durability, Entries, Op, StateError, StateStore};
use std::path::Path;
use std::sync::Mutex;

pub struct TursoStore {
    rt: tokio::runtime::Runtime,
    conn: Mutex<turso::Connection>,
    _db: turso::Database,
}

fn err(e: impl std::fmt::Display) -> StateError {
    StateError::Storage(e.to_string())
}

fn blob(v: turso::Value) -> Result<Vec<u8>, StateError> {
    match v {
        turso::Value::Blob(b) => Ok(b),
        other => Err(StateError::Storage(format!(
            "expected a blob, got {other:?}"
        ))),
    }
}

impl TursoStore {
    pub fn open(path: &Path) -> Result<Self, StateError> {
        Self::open_with(path, Durability::Durable)
    }

    /// `Durable` is Turso's default (fsync per commit); `Relaxed` asks for
    /// `PRAGMA synchronous=NORMAL`, if this Turso version honours it.
    pub fn open_with(path: &Path, durability: Durability) -> Result<Self, StateError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(err)?;
        let (db, conn) = rt.block_on(async {
            let db = turso::Builder::new_local(&path.to_string_lossy())
                .build()
                .await
                .map_err(err)?;
            let conn = db.connect().map_err(err)?;
            conn.execute(SCHEMA_ROWID, ()).await.map_err(err)?;
            if durability == Durability::Relaxed {
                // Not every Turso version has it; a refusal keeps the default.
                let _ = conn.execute("PRAGMA synchronous=NORMAL", ()).await;
            }
            Ok::<_, StateError>((db, conn))
        })?;
        Ok(Self {
            rt,
            conn: Mutex::new(conn),
            _db: db,
        })
    }

    async fn rows(
        conn: &turso::Connection,
        sql: &str,
        params: Vec<turso::Value>,
    ) -> Result<Entries, StateError> {
        let mut rows = conn.query(sql, params).await.map_err(err)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(err)? {
            out.push((
                blob(row.get_value(0).map_err(err)?)?,
                blob(row.get_value(1).map_err(err)?)?,
            ));
        }
        Ok(out)
    }
}

impl StateStore for TursoStore {
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        let conn = self.conn.lock().unwrap();
        self.rt.block_on(async {
            let mut rows = conn
                .query(
                    "SELECT v FROM kv WHERE t = ?1 AND k = ?2",
                    vec![
                        turso::Value::Text(table.into()),
                        turso::Value::Blob(key.to_vec()),
                    ],
                )
                .await
                .map_err(err)?;
            match rows.next().await.map_err(err)? {
                Some(row) => Ok(Some(blob(row.get_value(0).map_err(err)?)?)),
                None => Ok(None),
            }
        })
    }

    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError> {
        let conn = self.conn.lock().unwrap();
        let t = turso::Value::Text(table.into());
        let p = turso::Value::Blob(prefix.to_vec());
        self.rt.block_on(async {
            match prefix_end(prefix) {
                Some(end) => {
                    Self::rows(
                        &conn,
                        "SELECT k, v FROM kv WHERE t = ?1 AND k >= ?2 AND k < ?3 ORDER BY k",
                        vec![t, p, turso::Value::Blob(end)],
                    )
                    .await
                }
                None => {
                    Self::rows(
                        &conn,
                        "SELECT k, v FROM kv WHERE t = ?1 AND k >= ?2 ORDER BY k",
                        vec![t, p],
                    )
                    .await
                }
            }
        })
    }

    fn write(&self, batch: Batch) -> Result<(), StateError> {
        let conn = self.conn.lock().unwrap();
        self.rt.block_on(async {
            conn.execute("BEGIN", ()).await.map_err(err)?;
            let run = async {
                for op in &batch.ops {
                    match op {
                        Op::Put { table, key, value } => conn
                            .execute(
                                "INSERT INTO kv (t, k, v) VALUES (?1, ?2, ?3) ON CONFLICT (t, k) DO UPDATE SET v = excluded.v",
                                vec![
                                    turso::Value::Text(table.clone()),
                                    turso::Value::Blob(key.clone()),
                                    turso::Value::Blob(value.clone()),
                                ],
                            )
                            .await
                            .map_err(err)?,
                        Op::Delete { table, key } => conn
                            .execute(
                                "DELETE FROM kv WHERE t = ?1 AND k = ?2",
                                vec![turso::Value::Text(table.clone()), turso::Value::Blob(key.clone())],
                            )
                            .await
                            .map_err(err)?,
                    };
                }
                Ok::<_, StateError>(())
            };
            match run.await {
                Ok(()) => conn.execute("COMMIT", ()).await.map(|_| ()).map_err(err),
                Err(e) => {
                    let _ = conn.execute("ROLLBACK", ()).await;
                    Err(e)
                }
            }
        })
    }
}
