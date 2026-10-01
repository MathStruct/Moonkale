//! # moonkale-sources-kv
//!
//! Key/value sources. **Milestone 17**: the embedded stores **redb** (one
//! file, pure Rust) and **RocksDB** (a directory, C++), browsable through
//! [`KvSource`] — tables in the Sources tree, the `kv` dialect in the table
//! editor ([`kv`]). Native only, each behind its feature.
//!
//! Planned, designed only: Redis and Dragonfly, with key patterns turned into
//! a synthetic hierarchy — see the vault's `architecture/Data Sources.md`
//! ("Driver designs not built yet").

pub mod kv;

#[cfg(all(
    any(feature = "redb", feature = "rocksdb"),
    not(target_arch = "wasm32")
))]
mod source;
#[cfg(all(
    any(feature = "redb", feature = "rocksdb"),
    not(target_arch = "wasm32")
))]
pub use source::KvSource;

#[cfg(all(feature = "redb", not(target_arch = "wasm32")))]
pub mod redb_store;
#[cfg(all(feature = "rocksdb", not(target_arch = "wasm32")))]
pub mod rocksdb_store;

/// A redb database file. Path check only, usable on every target.
pub fn is_redb_path(path: &str) -> bool {
    ext_is(path, "redb")
}

/// A RocksDB database directory, by name (`*.rocksdb`) — `.rdb` would be a
/// Redis dump. Path check only, usable on every target.
pub fn is_rocksdb_path(path: &str) -> bool {
    ext_is(path.trim_end_matches('/'), "rocksdb")
}

fn ext_is(path: &str, ext: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(stem, e)| {
        !stem.is_empty() && !stem.ends_with('/') && e.eq_ignore_ascii_case(ext)
    })
}

/// Open a redb file as a source.
#[cfg(all(feature = "redb", not(target_arch = "wasm32")))]
pub fn open_redb(
    path: &std::path::Path,
) -> Result<KvSource<redb_store::RedbStore>, moonkale_core::SourceError> {
    let path = std::fs::canonicalize(path)?;
    let store = redb_store::RedbStore::open(&path).map_err(moonkale_core::SourceError::Io)?;
    Ok(KvSource::new("redb", &path, store))
}

/// Open a RocksDB directory as a source (a secondary instance).
#[cfg(all(feature = "rocksdb", not(target_arch = "wasm32")))]
pub fn open_rocksdb(
    path: &std::path::Path,
) -> Result<KvSource<rocksdb_store::RocksStore>, moonkale_core::SourceError> {
    let path = std::fs::canonicalize(path)?;
    let store = rocksdb_store::RocksStore::open(&path).map_err(moonkale_core::SourceError::Io)?;
    Ok(KvSource::new("rocksdb", &path, store))
}

#[cfg(test)]
mod path_tests {
    #[test]
    fn names() {
        assert!(super::is_redb_path("data/app.redb"));
        assert!(super::is_rocksdb_path("state.rocksdb/"));
        assert!(!super::is_rocksdb_path("dump.rdb"));
        assert!(!super::is_redb_path(".redb"));
    }
}
