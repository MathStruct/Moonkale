//! The state store of the machine that hosts the folders (Milestone 18 phase
//! 5.3, ADR-0014): the server's `state.sqlite` in its config directory, for
//! the state that belongs with a folder rather than with a client — the
//! entity log today. A web page, a desktop connected to a server and an SSH
//! remote all reach it through these functions; a desktop's own folders use
//! its own store (`client::host_state_routed`).
//!
//! Only the host's tables ([`HOST_TABLES`]) are reachable, and only under the
//! id of a folder this server has open: a key's first part must be one.
//! Keys and values travel base64.

use base64::Engine;
use dioxus::prelude::*;

/// The tables a client may read and write on the host: the entity log, and
/// a client's own (`"local"`) agent sessions — the server's sessions, under
/// `"server"`, only the server writes (`agent_sessions`).
pub const HOST_TABLES: &[&str] = &[
    moonkale_state::tables::EVENTS,
    moonkale_state::tables::AGENT_SESSIONS,
];

pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| e.to_string())
}

/// One operation of a batch on the wire: `(table, key, Some(value) | None =
/// delete)`, key and value base64.
pub type WireOp = (String, String, Option<String>);

pub fn to_wire(batch: moonkale_state::Batch) -> Vec<WireOp> {
    batch
        .ops
        .into_iter()
        .map(|op| match op {
            moonkale_state::Op::Put { table, key, value } => (table, b64(&key), Some(b64(&value))),
            moonkale_state::Op::Delete { table, key } => (table, b64(&key), None),
        })
        .collect()
}

pub fn from_wire(ops: Vec<WireOp>) -> Result<moonkale_state::Batch, String> {
    let mut batch = moonkale_state::Batch::new();
    for (table, key, value) in ops {
        let key = unb64(&key)?;
        batch = match value {
            Some(v) => batch.put(&table, key, unb64(&v)?),
            None => batch.delete(&table, key),
        };
    }
    Ok(batch)
}

#[cfg(feature = "server")]
pub(crate) mod server {
    use std::sync::{Arc, OnceLock};

    pub fn store() -> Result<&'static Arc<dyn moonkale_state::StateStore>, String> {
        static STORE: OnceLock<Result<Arc<dyn moonkale_state::StateStore>, String>> =
            OnceLock::new();
        STORE
            .get_or_init(|| {
                let dir = moonkale_llm::secrets::config_dir()
                    .ok_or_else(|| "no config directory".to_string())?;
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let store = moonkale_state_stores::SqliteStore::open_with(
                    &dir.join("state.sqlite"),
                    moonkale_state::Durability::Relaxed,
                )
                .map_err(|e| e.to_string())?;
                Ok(Arc::new(store) as Arc<dyn moonkale_state::StateStore>)
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// A host table, under a folder this server has open.
    pub fn check(table: &str, key: &[u8]) -> Result<(), String> {
        if !super::HOST_TABLES.contains(&table) {
            return Err(format!("table {table:?} is not the host's"));
        }
        let mut parts = moonkale_state::Key::reader(key);
        let folder = parts
            .str()
            .ok_or_else(|| "a key must start with a folder id".to_string())?;
        if table == moonkale_state::tables::AGENT_SESSIONS
            && parts.str().as_deref() != Some("local")
        {
            return Err("only a client's own (local) agent sessions".into());
        }
        if crate::state::registry()
            .get(&moonkale_core::SourceId::new(folder.clone()))
            .is_none()
        {
            return Err(format!("{folder} is not open on this server"));
        }
        Ok(())
    }
}

/// `(table, key)` → the value, if any.
#[post("/api/state/get")]
pub async fn host_state_get(table: String, key: String) -> Result<Option<String>, ServerFnError> {
    let key = unb64(&key).map_err(ServerFnError::new)?;
    server::check(&table, &key).map_err(ServerFnError::new)?;
    let value = server::store()
        .map_err(ServerFnError::new)?
        .get(&table, &key)
        .map_err(ServerFnError::new)?;
    Ok(value.as_deref().map(b64))
}

/// The entries under a prefix (which must name a folder), in key order.
#[post("/api/state/scan")]
pub async fn host_state_scan(
    table: String,
    prefix: String,
) -> Result<Vec<(String, String)>, ServerFnError> {
    let prefix = unb64(&prefix).map_err(ServerFnError::new)?;
    server::check(&table, &prefix).map_err(ServerFnError::new)?;
    let rows = server::store()
        .map_err(ServerFnError::new)?
        .scan(&table, &prefix)
        .map_err(ServerFnError::new)?;
    Ok(rows.iter().map(|(k, v)| (b64(k), b64(v))).collect())
}

/// Apply a batch, all or nothing.
#[post("/api/state/write")]
pub async fn host_state_write(ops: Vec<WireOp>) -> Result<(), ServerFnError> {
    let batch = from_wire(ops).map_err(ServerFnError::new)?;
    for op in &batch.ops {
        let (moonkale_state::Op::Put { table, key, .. }
        | moonkale_state::Op::Delete { table, key }) = op;
        server::check(table, key).map_err(ServerFnError::new)?;
    }
    server::store()
        .map_err(ServerFnError::new)?
        .write(batch)
        .map_err(ServerFnError::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_survive_the_wire() {
        let batch = moonkale_state::Batch::new()
            .put("events", vec![0, 1, 255], vec![7; 300])
            .delete("events", vec![9]);
        assert_eq!(from_wire(to_wire(batch.clone())).unwrap(), batch);
    }
}
