//! Connection lifecycle (design note — Milestone 1 sources are opened synchronously and never reconnect).
//!
//! Planned: `ConnectParams` → `SourceFactory::connect` → `ConnectionState` transitions (Connecting → Ready ↔ Degraded → Closed) with backoff; the UI draws the status dot from it.
