use super::{ContentIdentity, NORMALIZER_VERSION};
use moonkale_core::NodeId;
use moonkale_state::{Batch, StateStore};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

const PREFIX: &[u8] = b"moonkale-content-v1:";
const TABLE: &str = moonkale_state::tables::INDEX;
const MAX_ENTRY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Record {
    key: Vec<u8>,
    identities: HashMap<NodeId, ContentIdentity>,
    touched: u128,
}

/// Bounded memory LRU. Optional persistent records share the same byte budget;
/// cache errors and oversized/corrupt records are misses, never source errors.
pub(crate) struct Cache {
    values: HashMap<Vec<u8>, Vec<u8>>,
    order: VecDeque<Vec<u8>>,
    bytes: usize,
    budget: usize,
    store: Option<Arc<dyn StateStore>>,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            budget: if cfg!(target_os = "android") { 4 } else { 16 } * 1024 * 1024,
            store: None,
        }
    }
}
impl Cache {
    pub(crate) fn set_store(&mut self, store: Arc<dyn StateStore>) {
        self.store = Some(store);
    }
    pub(crate) fn key(input: &[u8; 32], selector: &str) -> Vec<u8> {
        let mut bytes = PREFIX.to_vec();
        bytes.extend_from_slice(input);
        bytes.extend_from_slice(&NORMALIZER_VERSION.to_le_bytes());
        bytes.extend_from_slice(blake3::hash(selector.as_bytes()).as_bytes());
        bytes
    }
    pub(crate) fn get(&mut self, key: &[u8]) -> Option<HashMap<NodeId, ContentIdentity>> {
        let bytes = self
            .values
            .get(key)
            .cloned()
            .or_else(|| self.store.as_ref()?.get(TABLE, key).ok().flatten())?;
        if bytes.len() > MAX_ENTRY_BYTES {
            return None;
        }
        let mut record = decode(&bytes)?;
        if record.key != key
            || record.identities.is_empty()
            || record.identities.len() > super::MAX_DEFINITIONS
        {
            return None;
        }
        if record.identities.values().any(|id| {
            id.normalizer_version != NORMALIZER_VERSION
                || id.id != NodeId::from_content(&id.digest)
                || id.component_size == 0
                || id.component_size as usize > super::MAX_DEFINITIONS
                || id.component_index >= id.component_size
        }) {
            return None;
        }
        record.touched = now();
        let bytes = encode(&record)?;
        self.persist(key, &bytes);
        self.remember(key.to_vec(), bytes);
        Some(record.identities)
    }
    pub(crate) fn put(&mut self, key: Vec<u8>, identities: HashMap<NodeId, ContentIdentity>) {
        let Some(bytes) = encode(&Record {
            key: key.clone(),
            identities,
            touched: now(),
        }) else {
            return;
        };
        if bytes.len() > MAX_ENTRY_BYTES || bytes.len() + key.len() > self.budget {
            return;
        }
        self.persist(&key, &bytes);
        self.remember(key, bytes);
    }
    fn persist(&self, key: &[u8], bytes: &[u8]) {
        static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = GATE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(store) = &self.store {
            if let Ok(mut entries) = store.scan(TABLE, PREFIX) {
                entries.sort_by_key(|(_, value)| decode(value).map(|r| r.touched).unwrap_or(0));
                let mut size: usize = entries
                    .iter()
                    .filter(|(k, _)| k != key)
                    .map(|(k, v)| k.len() + v.len())
                    .sum();
                let mut batch = Batch::new();
                for (old, value) in entries {
                    if old != key && size + key.len() + bytes.len() > self.budget {
                        size = size.saturating_sub(old.len() + value.len());
                        batch = batch.delete(TABLE, old);
                    }
                }
                let _ = store.write(batch.put(TABLE, key.to_vec(), bytes.to_vec()));
            }
        }
    }
    fn remember(&mut self, key: Vec<u8>, bytes: Vec<u8>) {
        if let Some(old) = self.values.remove(&key) {
            self.bytes -= old.len() + key.len();
        }
        self.order.retain(|k| k != &key);
        if bytes.len() + key.len() > self.budget {
            return;
        }
        while self.bytes + bytes.len() + key.len() > self.budget {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(value) = self.values.remove(&old) {
                self.bytes -= old.len() + value.len();
            }
        }
        self.bytes += bytes.len() + key.len();
        self.order.push_back(key.clone());
        self.values.insert(key, bytes);
    }
}

fn now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
fn encode(record: &Record) -> Option<Vec<u8>> {
    let payload = serde_json::to_vec(record).ok()?;
    let mut bytes = blake3::hash(&payload).as_bytes().to_vec();
    bytes.extend_from_slice(&payload);
    Some(bytes)
}
fn decode(bytes: &[u8]) -> Option<Record> {
    if bytes.len() > MAX_ENTRY_BYTES || bytes.len() < 32 {
        return None;
    }
    let (digest, payload) = bytes.split_at(32);
    if digest != blake3::hash(payload).as_bytes() {
        return None;
    }
    serde_json::from_slice(payload).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(n: u8) -> HashMap<NodeId, ContentIdentity> {
        let digest = [n; 32];
        let id = NodeId::from_content(&digest);
        HashMap::from([(
            id,
            ContentIdentity {
                id,
                digest,
                normalizer_version: NORMALIZER_VERSION,
                component_digest: [1; 32],
                component_index: 0,
                component_size: 1,
            },
        )])
    }
    #[test]
    fn memory_lru_evicts_the_least_recently_read_and_honors_bytes() {
        let mut cache = Cache::default();
        let a = Cache::key(&[1; 32], "a");
        let b = Cache::key(&[2; 32], "b");
        let c = Cache::key(&[3; 32], "c");
        cache.put(a.clone(), record(1));
        let one = cache.bytes;
        cache.budget = one * 2 + 256;
        cache.put(b.clone(), record(2));
        assert!(cache.get(&a).is_some());
        cache.put(c.clone(), record(3));
        assert!(cache.get(&b).is_none());
        assert!(cache.get(&a).is_some());
        assert!(cache.get(&c).is_some());
        assert!(cache.bytes <= cache.budget);
    }
    #[test]
    fn persisted_reopen_corruption_and_version_mismatch_are_safe() {
        let store = Arc::new(moonkale_state::MemoryStore::new());
        let key = Cache::key(&[1; 32], "a");
        let mut first = Cache::default();
        first.set_store(store.clone());
        first.put(key.clone(), record(1));
        let mut second = Cache::default();
        second.set_store(store.clone());
        assert_eq!(second.get(&key), Some(record(1)));
        store
            .write(Batch::new().put(TABLE, key.clone(), b"broken".to_vec()))
            .unwrap();
        let mut third = Cache::default();
        third.set_store(store.clone());
        assert!(third.get(&key).is_none());
        let mut wrong = record(1);
        wrong.values_mut().next().unwrap().normalizer_version += 1;
        first.put(key.clone(), wrong);
        let mut fourth = Cache::default();
        fourth.set_store(store);
        assert!(fourth.get(&key).is_none());
    }
}
