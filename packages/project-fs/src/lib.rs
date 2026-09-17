//! # moonkale-project-fs
//!
//! "Open a folder" as a `Source`. Directories are `Directory` nodes, files
//! are `File` nodes with `ContentRef::Text`/`Blob`, containment is the edge.
//! Markdown wiki-links and code references are *not* extracted here — that
//! is `moonkale-index`'s job — this crate only knows about bytes and paths.
//!
//! **Milestone 1** implements the native backend (`std::fs`/`tokio::fs`):
//! listing (`.gitignore`-aware via the `ignore` crate), reading text, and
//! atomic patch-writes with optimistic concurrency. File *watching* and the
//! web (OPFS / File System Access) and mobile backends are still design
//! notes in [`watch`] and [`platform`].
//!
//! The crate compiles on wasm32 to an empty shell so that workspace-wide
//! checks pass; the `web` crate must never depend on it.

pub mod platform;
#[cfg(not(target_arch = "wasm32"))]
pub mod source;
#[cfg(not(target_arch = "wasm32"))]
pub mod tree;
pub mod watch;

#[cfg(not(target_arch = "wasm32"))]
pub use source::FolderSource;
