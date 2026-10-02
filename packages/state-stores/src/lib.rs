//! # moonkale-state-stores
//!
//! The engines behind [`moonkale_state::StateStore`] (ADR-0014). The app uses
//! **SQLite** ([`SqliteStore`], feature `sqlite`); redb, Turso, RocksDB and
//! HelixDB stay here from the comparison of Milestone 18 phase 5
//! (`examples/compare.rs`, vault `research/State Store Comparison.md`), so a
//! later switch is a feature and a [`moonkale_state::copy`].
//! Split from `moonkale-state` so the extension contract can depend on the
//! interface without the engines.

#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
mod helix_store;
#[cfg(all(feature = "redb", not(target_arch = "wasm32")))]
mod redb_store;
#[cfg(all(feature = "rocksdb", not(target_arch = "wasm32")))]
mod rocks_store;
#[allow(dead_code)]
mod sql;
#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
mod sqlite_store;
#[cfg(all(feature = "turso", not(target_arch = "wasm32")))]
mod turso_store;

#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
pub use helix_store::HelixStore;
#[cfg(all(feature = "redb", not(target_arch = "wasm32")))]
pub use redb_store::RedbStore;
#[cfg(all(feature = "rocksdb", not(target_arch = "wasm32")))]
pub use rocks_store::RocksStore;
#[cfg(all(feature = "sqlite", not(target_arch = "wasm32")))]
pub use sqlite_store::SqliteStore;
#[cfg(all(feature = "turso", not(target_arch = "wasm32")))]
pub use turso_store::TursoStore;
