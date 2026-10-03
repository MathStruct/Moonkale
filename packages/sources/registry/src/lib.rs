//! # moonkale-sources
//!
//! Everything about sources that is *not* a database driver. Today that is
//! [`registry::SourceRegistry`]; the connection lifecycle, credentials and
//! shared lifting rules are designs in the vault (`architecture/Data
//! Sources.md`). Milestone 18 phase 2 adds the source openers here.
//!
//! **Decision (M1)**: the `RemoteSource` proxy lives in the `api` crate, not
//! here — it must call the server functions, and `api` must hold the
//! registry, so keeping the client stub next to its server functions avoids
//! a dependency cycle. See `sources.md`.

pub mod registry;

pub use registry::SourceRegistry;
