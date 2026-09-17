//! # moonkale-sources
//!
//! Everything about sources that is *not* a database driver:
//!
//! - the [`registry::SourceRegistry`] of open sources in a workspace,
//! - [`connect`]: the connection state machine (Connecting → Ready ↔ Degraded
//!   → Closed) and reconnection policy,
//! - [`credentials`]: where secrets live on each platform,
//! - [`lift`]: the shared rules for turning rows / vertices / keys into
//!   `core::graph` nodes and edges,
//! - [`remote`]: a `Source` implementation that proxies to the `api` server
//!   — this is the *only* source the web and mobile builds contain.
//!
//! Drivers live in `moonkale-sources-{sql,graph,kv}` and are native-only.
//! The split exists so the wasm build never even sees `libpq`.
//!
//! See the vault: `markdown/architecture/Data Sources.md`.

pub mod connect;
pub mod credentials;
pub mod lift;
pub mod registry;
pub mod remote;
