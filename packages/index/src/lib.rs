//! # moonkale-index
//!
//! The index is a *derived source*: it reads other sources and emits nodes
//! and edges they don't know about — symbols in code, wiki-links between
//! pages, headings, embeddings, stack-trace frames. It is what turns a folder
//! of files into a *graph* and is therefore central to the "graph-native"
//! promise.
//!
//! ```text
//!  SourceEvent ─► pipeline ─► parse ─► extract ─► store ─► IndexSource
//!                    ▲                                        │
//!                    └────────── incremental (per node) ──────┘
//! ```
//!
//! Extractors are pluggable (extensions may contribute one per language or
//! file kind). Everything is incremental: a changed node re-runs only its own
//! extractors and re-links only its own edges.
//!
//! Platform: parsing (tree-sitter grammars compiled to wasm) runs everywhere;
//! full-text and vector *storage* is native/server — the web build queries
//! the server's index.

pub mod embed;
pub mod extract;
pub mod parse;
pub mod pipeline;
pub mod store;
pub mod trace;
