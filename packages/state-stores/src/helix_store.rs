//! HelixDB, embedded (a graph + vector database over SlateDB) used as a
//! key/value store — the fourth candidate. There is no key/value API, so
//! each state table is a node label (`kv_<table>`), each entry a node with
//! the key as a hex string (`k`, which keeps byte order) and the value as
//! base64 (`v`). A batch is one Helix write batch (drop the old node, add
//! the new one). Scans fetch the table's nodes and filter and sort in Rust:
//! the query language has no prefix range. Async API, bridged with a
//! private Tokio runtime like [`crate::TursoStore`].

use base64::Engine;
use helix_db::dsl::prelude::*;
use helix_db::{Client, HelixDbSource, QueryRequest};
use moonkale_state::{Batch, Entries, Op, StateError, StateStore};
use serde_json::Value as Json;
use std::path::Path;

pub struct HelixStore {
    rt: tokio::runtime::Runtime,
    client: Client,
    /// Labels that have their equality index on `k` (created on first write).
    indexed: std::sync::Mutex<std::collections::HashSet<String>>,
}

fn err(e: impl std::fmt::Display) -> StateError {
    StateError::Storage(e.to_string())
}

fn label(table: &str) -> String {
    format!("kv_{table}")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, StateError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(err))
        .collect()
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

impl HelixStore {
    /// Open (or create) the store whose root directory is `path`.
    pub fn open(path: &Path) -> Result<Self, StateError> {
        std::fs::create_dir_all(path).map_err(err)?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(err)?;
        let client = rt.block_on(async {
            Client::open(HelixDbSource::Disk {
                root: path.to_path_buf(),
                database: "main".into(),
            })
            .await
            .map_err(err)
        })?;
        Ok(Self {
            rt,
            client,
            indexed: Default::default(),
        })
    }

    /// An equality index on `k`, so `get` and the drop before a put do not
    /// scan the whole label (without it, appends cost O(n) each — measured).
    fn ensure_index(&self, table: &str) -> Result<(), StateError> {
        let l = label(table);
        if self.indexed.lock().unwrap().contains(&l) {
            return Ok(());
        }
        let w = write_batch()
            .var_as(
                "ix",
                g().create_index_if_not_exists(IndexSpec::node_equality(l.clone(), "k")),
            )
            .returning(Vec::<String>::new());
        let _: Json = self
            .rt
            .block_on(async { self.client.query(QueryRequest::write(w)).send().await })
            .map_err(err)?;
        self.indexed.lock().unwrap().insert(l);
        Ok(())
    }

    fn entries(&self, table: &str) -> Result<Vec<(String, String)>, StateError> {
        let q = read_batch()
            .var_as(
                "r",
                g().n_with_label(label(table))
                    .value_map(Some(vec!["k".to_string(), "v".to_string()])),
            )
            .returning(["r"]);
        let r: Json = self
            .rt
            .block_on(async { self.client.query(QueryRequest::read(q)).send().await })
            .map_err(err)?;
        Ok(r.get("r")
            .and_then(Json::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|o| {
                        Some((
                            o.get("k")?.as_str()?.to_string(),
                            o.get("v")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

impl StateStore for HelixStore {
    fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StateError> {
        let want = hex(key);
        let q = read_batch()
            .var_as(
                "r",
                g().n_with_label(label(table))
                    .has("k", want.as_str())
                    .limit(1)
                    .value_map(Some(vec!["v".to_string()])),
            )
            .returning(["r"]);
        let r: Json = self
            .rt
            .block_on(async { self.client.query(QueryRequest::read(q)).send().await })
            .map_err(err)?;
        match r
            .get("r")
            .and_then(Json::as_array)
            .and_then(|a| a.first())
            .and_then(|o| o.get("v"))
            .and_then(Json::as_str)
        {
            Some(v) => Ok(Some(b64().decode(v).map_err(err)?)),
            None => Ok(None),
        }
    }

    fn scan(&self, table: &str, prefix: &[u8]) -> Result<Entries, StateError> {
        let p = hex(prefix);
        let mut rows: Vec<(String, String)> = self
            .entries(table)?
            .into_iter()
            .filter(|(k, _)| k.starts_with(&p))
            .collect();
        rows.sort();
        rows.into_iter()
            .map(|(k, v)| Ok((unhex(&k)?, b64().decode(v).map_err(err)?)))
            .collect()
    }

    fn write(&self, batch: Batch) -> Result<(), StateError> {
        if batch.is_empty() {
            return Ok(());
        }
        for op in &batch.ops {
            match op {
                Op::Put { table, .. } | Op::Delete { table, .. } => self.ensure_index(table)?,
            }
        }
        let mut w = write_batch();
        for (i, op) in batch.ops.iter().enumerate() {
            let (table, key) = match op {
                Op::Put { table, key, .. } | Op::Delete { table, key } => (table, key),
            };
            let k = hex(key);
            let (dn, an) = (format!("d{i}"), format!("a{i}"));
            w = w.var_as(
                &dn,
                g().n_with_label(label(table)).has("k", k.as_str()).drop(),
            );
            if let Op::Put { value, .. } = op {
                w = w.var_as(
                    &an,
                    g().add_n(
                        label(table),
                        vec![("k", k.clone()), ("v", b64().encode(value))],
                    ),
                );
            }
        }
        let w = w.returning(Vec::<String>::new());
        // Helix's transactions are optimistic: a write that overlaps a
        // concurrent read can fail with "transaction conflict" (found by the
        // conformance suite's reader-during-writer check). Retry, bounded.
        let mut attempt = 0;
        loop {
            let r: Result<Json, _> = self.rt.block_on(async {
                self.client
                    .query(QueryRequest::write(w.clone()))
                    .send()
                    .await
            });
            match r {
                Ok(_) => return Ok(()),
                Err(e) if attempt < 50 && e.to_string().contains("conflict") => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(2 * attempt));
                }
                Err(e) => return Err(err(e)),
            }
        }
    }
}
