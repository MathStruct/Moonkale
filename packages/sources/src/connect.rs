//! Connection lifecycle.
//!
//! `ConnectParams` (dialect + host/port/db or file path + options) →
//! `SourceFactory::connect` → `ConnectionState` transitions with backoff.
//! Health checks are dialect-specific and provided by the driver crates.
//! The UI subscribes to state changes to draw the status dot.
