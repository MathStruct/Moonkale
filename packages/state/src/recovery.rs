//! Bounded recovery records (ADR-0026). Source text remains source-owned.
use crate::{Batch, Entries, Key, Record};
use serde::{Deserialize, Serialize};

/// Platform defaults, in bytes. Recovery is separate from rebuildable data.
#[derive(Clone, Copy, Debug)]
pub struct Budgets {
    pub buffer: usize,
    pub recovery: usize,
    pub derived: usize,
    pub math: usize,
    pub images: usize,
    pub folder: usize,
}
impl Budgets {
    pub const fn platform() -> Self {
        if cfg!(target_arch = "wasm32") {
            Self {
                buffer: 128 * 1024,
                recovery: 512 * 1024,
                derived: 4 * 1024 * 1024,
                math: 2 * 1024 * 1024,
                images: 16 * 1024 * 1024,
                folder: 64 * 1024 * 1024,
            }
        } else if cfg!(target_os = "android") {
            Self {
                buffer: 512 * 1024,
                recovery: 4 * 1024 * 1024,
                derived: 4 * 1024 * 1024,
                math: 2 * 1024 * 1024,
                images: 16 * 1024 * 1024,
                folder: 64 * 1024 * 1024,
            }
        } else {
            Self {
                buffer: 2 * 1024 * 1024,
                recovery: 16 * 1024 * 1024,
                derived: 16 * 1024 * 1024,
                math: 8 * 1024 * 1024,
                images: 64 * 1024 * 1024,
                folder: 256 * 1024 * 1024,
            }
        }
    }
}

/// One unsaved buffer, identified independently of transient node ids.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub document: String,
    pub base_version: String,
    pub base: String,
    pub text: String,
    pub touched: u64,
}
impl Record for Snapshot {
    const TABLE: &'static str = crate::tables::RECOVERY;
    const VERSION: u32 = 1;
}
impl Snapshot {
    pub fn prefix(&self) -> Key {
        Key::new().str(&self.source).str(&self.document)
    }
    pub fn key(&self) -> Key {
        self.prefix().str(&self.base_version)
    }
    pub fn admissible(&self, budget: Budgets) -> bool {
        allowed_path(&self.document)
            && self.source.len() <= 4096
            && self.document.len() <= 4096
            && self.base_version.len() <= 128
            && self.base.len().saturating_add(self.text.len()) <= budget.buffer
    }
}

/// Conservative exclusion of hidden paths, credentials and key material.
/// Callers additionally admit only source-enumerated folder files.
pub fn allowed_path(path: &str) -> bool {
    !path.is_empty()
        && !path.split(['/', '\\']).any(|part| {
            let p = part.to_ascii_lowercase();
            p.is_empty()
                || p.starts_with('.')
                || p == ".."
                || matches!(
                    p.as_str(),
                    "secrets.json" | "credentials" | "credentials.json" | "id_rsa" | "id_ed25519"
                )
                || [".pem", ".key", ".p12", ".pfx", ".keystore"]
                    .iter()
                    .any(|suffix| p.ends_with(suffix))
        })
}

/// Replace a document's old versions and evict the oldest snapshots until the
/// encoded keys and values fit. Call scan + plan + write serially per client.
/// Oversized replacements remove old snapshots rather than leaving stale work.
pub fn replace(entries: Entries, snapshot: &Snapshot, budget: Budgets) -> Batch {
    let mut batch = Batch::new();
    let prefix = snapshot.prefix();
    let mut remaining = Vec::new();
    for (key, value) in entries {
        if key.starts_with(prefix.as_bytes()) {
            batch = batch.delete(Snapshot::TABLE, key);
        } else {
            let touched = if value.len() <= budget.buffer.saturating_mul(6).saturating_add(32768) {
                crate::decode::<Snapshot>(&value)
                    .ok()
                    .map(|s| s.touched)
                    .unwrap_or(0)
            } else {
                0
            };
            remaining.push((key, value.len(), touched));
        }
    }
    if !snapshot.admissible(budget) || snapshot.base == snapshot.text {
        return batch;
    }
    let value = crate::encode(snapshot);
    let key = snapshot.key();
    let size = key.as_bytes().len() + value.len();
    if !snapshot.admissible(budget) || size > budget.recovery || snapshot.base == snapshot.text {
        return batch;
    }
    remaining.sort_by_key(|(_, _, touched)| *touched);
    let mut total = remaining
        .iter()
        .fold(size, |sum, (k, len, _)| sum.saturating_add(k.len() + len));
    for (old, len, _) in remaining {
        if total <= budget.recovery {
            break;
        }
        total = total.saturating_sub(old.len() + len);
        batch = batch.delete(Snapshot::TABLE, old);
    }
    batch.put(Snapshot::TABLE, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryStore, StateStore};
    fn snapshot(source: &str, path: &str, version: &str, touched: u64) -> Snapshot {
        Snapshot {
            source: source.into(),
            document: path.into(),
            base_version: version.into(),
            base: "base".into(),
            text: "edited".into(),
            touched,
        }
    }
    #[test]
    fn keys_isolate_sources_paths_and_versions() {
        let a = snapshot("a", "file", "1", 1);
        assert_ne!(a.key(), snapshot("ab", "file", "1", 1).key());
        assert_ne!(a.key(), snapshot("a", "files", "1", 1).key());
        assert_ne!(a.key(), snapshot("a", "file", "2", 1).key());
        let store = MemoryStore::new();
        store
            .write(replace(vec![], &a, Budgets::platform()))
            .unwrap();
        let b = snapshot("a", "file", "2", 2);
        store
            .write(replace(
                store.scan(Snapshot::TABLE, &[]).unwrap(),
                &b,
                Budgets::platform(),
            ))
            .unwrap();
        assert!(store
            .get(Snapshot::TABLE, a.key().as_bytes())
            .unwrap()
            .is_none());
        assert!(store
            .get(Snapshot::TABLE, b.key().as_bytes())
            .unwrap()
            .is_some());
    }
    #[test]
    fn bounds_and_oldest_eviction_include_encoded_bytes() {
        let store = MemoryStore::new();
        let a = snapshot("a", "one", "1", 1);
        let b = snapshot("a", "two", "1", 2);
        let budget = Budgets {
            buffer: 64,
            recovery: crate::encode(&a).len() + a.key().as_bytes().len(),
            derived: 0,
            ..Budgets::platform()
        };
        for s in [&a, &b] {
            store
                .write(replace(
                    store.scan(Snapshot::TABLE, &[]).unwrap(),
                    s,
                    budget,
                ))
                .unwrap();
        }
        let rows = store.scan(Snapshot::TABLE, &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, b.key().into_bytes());
        let mut huge = b.clone();
        huge.text = "x".repeat(65);
        store.write(replace(rows, &huge, budget)).unwrap();
        assert!(store.scan(Snapshot::TABLE, &[]).unwrap().is_empty());
    }
    #[test]
    fn sensitive_paths_are_never_admitted() {
        for path in [
            ".env",
            "x/.env.local",
            ".secrets/token",
            "secrets.json",
            "x/id_rsa",
            "cert.pem",
            "x/../file",
            "/absolute",
        ] {
            assert!(!allowed_path(path), "{path}");
        }
        assert!(allowed_path("src/main.rs"));
    }
}
