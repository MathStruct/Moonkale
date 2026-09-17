---
title: "sources — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `moonkale-sources` (Milestone 1). Design: [[Data Sources]].

`registry.rs` is the only implemented module: `SourceRegistry` = `RwLock<HashMap<SourceId, Arc<dyn Source>>>` with `insert/get/remove/descriptors`. The server (`api`) holds one as a process-wide `OnceLock`.

**Decision:** `RemoteSource` was planned here but lives in `api` — it calls `api`'s server functions and `api` needs this registry; the same crate avoids a cycle. When there is a second remote transport this can be revisited.

`connect.rs`, `credentials.rs`, `lift.rs` remain design notes.
