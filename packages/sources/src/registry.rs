//! `SourceRegistry` — the open sources of a workspace, by `SourceId`.
//!
//! Deliberately synchronous and tiny: a `RwLock<HashMap>` of `Arc<dyn
//! Source>`. On the server it is a process-wide singleton; in the client it
//! is owned by the workspace. The multiplexing/fan-out described in the vault
//! (queries without a source filter) is not needed until two sources can be
//! open at once.

use moonkale_core::{Source, SourceDescriptor, SourceId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct SourceRegistry {
    sources: RwLock<HashMap<SourceId, Arc<dyn Source>>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) a source; returns its descriptor.
    pub fn insert(&self, source: Arc<dyn Source>) -> SourceDescriptor {
        let d = source.descriptor();
        self.sources.write().unwrap().insert(d.id.clone(), source);
        d
    }

    pub fn get(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
        self.sources.read().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &SourceId) -> Option<Arc<dyn Source>> {
        self.sources.write().unwrap().remove(id)
    }

    pub fn descriptors(&self) -> Vec<SourceDescriptor> {
        let mut v: Vec<_> = self
            .sources
            .read()
            .unwrap()
            .values()
            .map(|s| s.descriptor())
            .collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }
}
