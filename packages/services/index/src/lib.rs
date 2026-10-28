//! # moonkale-index
//!
//! The index is a *derived source*: it reads other sources and emits nodes
//! and edges they don't know about — symbols in code, wiki-links between
//! pages. It is what turns a folder of files into a *graph*.
//!
//! ```text
//!  folder source ─► walk ─► extract (wikilinks, symbols_rust) ─► IndexGraph ─► IndexSource
//!                                          ▲                          │
//!                                          └──── refresh(file) ───────┘
//! ```
//!
//! **Milestone 2**: in-memory graph rebuilt on open; two extractors; the
//! `IndexSource` answers `Node`, `Children`, `Neighbours`, `All`. Native only
//! (tree-sitter's C runtime does not target `wasm32-unknown-unknown`): on
//! desktop it runs in-process, on web it runs on the server and the client
//! sees it as a `RemoteSource`. Persistence, embeddings, LSP symbols and
//! stack traces remain design notes in the vault (`architecture/Indexing.md`).

#[cfg(not(target_arch = "wasm32"))]
pub mod extract;
#[cfg(not(target_arch = "wasm32"))]
pub mod graph;
#[cfg(not(target_arch = "wasm32"))]
pub mod search;
#[cfg(not(target_arch = "wasm32"))]
pub mod source;
#[cfg(not(target_arch = "wasm32"))]
pub mod walk;

#[cfg(not(target_arch = "wasm32"))]
pub use source::{IndexSource, IndexStats};

/// Lazy, versioned syntactic identities (WP-66); disabled by default.
#[cfg(all(feature = "content-ids", not(target_arch = "wasm32")))]
pub mod content;

/// Exact Rust syntax snapshots and guarded, pure structural patch planning.
#[cfg(all(feature = "lossless-syntax", not(target_arch = "wasm32")))]
pub mod lossless;

/// Derived per-folder Sophia writer (ADR-0029), disabled by default.
#[cfg(all(feature = "sophia", not(target_arch = "wasm32")))]
pub mod folder_store;

#[cfg(all(feature = "sophia", not(target_arch = "wasm32")))]
mod selection;
