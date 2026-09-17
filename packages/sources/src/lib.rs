//! # moonkale-sources
//!
//! Everything about sources that is *not* a database driver. Milestone 1
//! implements [`registry::SourceRegistry`]; [`connect`], [`credentials`] and
//! [`lift`] remain design notes.
//!
//! **Decision (M1)**: the `RemoteSource` proxy lives in the `api` crate, not
//! here — it must call the server functions, and `api` must hold the
//! registry, so keeping the client stub next to its server functions avoids
//! a dependency cycle. See `sources.md`.

pub mod connect;
pub mod credentials;
pub mod lift;
pub mod registry;

pub use registry::SourceRegistry;
