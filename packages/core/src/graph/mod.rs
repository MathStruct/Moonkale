//! The graph-native document model.
//!
//! ```text
//!            ┌──────────┐   Edge(kind: Contains)   ┌──────────┐
//!            │  Node    │ ───────────────────────► │  Node    │
//!            │ kind:Dir │                          │ kind:File│
//!            └──────────┘                          └──────────┘
//!                                                       │ Edge(kind: Defines)
//!                                                       ▼
//!                                                  ┌──────────┐
//!                                                  │  Node    │
//!                                                  │kind:Symbol
//!                                                  └──────────┘
//! ```
//!
//! Files, directories, table rows, database vertices, wiki pages, symbols,
//! flow-diagram blocks, ML layers — all are `Node`s. Containment, references,
//! foreign keys, wiki-links, "calls", "is-a" — all are `Edge`s. This is the one
//! big bet of the project: a single model that every editor and every source
//! speaks.
//!
//! What is deliberately *not* here: bytes. A `Node` of kind `File` has a
//! `content: ContentRef`, not a `String`. Content is fetched lazily through
//! the `source` module so that a 10 GB folder can be *opened* without being
//! *loaded*.

pub mod edge;
pub mod history;
pub mod node;
pub mod property;
pub mod view;

pub use edge::{Edge, EdgeKind};
pub use node::{ContentRef, Node, NodeKind, Version};
pub use property::Value;
