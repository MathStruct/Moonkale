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

pub use edge::{Edge, EdgeKind};
pub use history::{Actor, EntityLog, Event, EventId, EventKind, State as HistoryState};
pub use node::{ContentRef, Node, NodeKind, Version};
pub use property::{Properties, Value};

/// Shared L2 vocabulary. Use with `NodeKind::Custom` / `EdgeKind::Custom`;
/// legacy built-in kinds retain their wire representation.
pub mod code {
    /// A language definition.
    pub const DEFINITION: &str = "Definition";
    /// A namespace name.
    pub const NAME: &str = "Name";
    /// A reference without a unique target.
    pub const UNRESOLVED: &str = "Unresolved";
    /// Name to definition.
    pub const NAMES: &str = "Names";
    /// Declared or inferred type.
    pub const TYPE_OF: &str = "TypeOf";
    /// Implementation of an interface.
    pub const IMPLEMENTS: &str = "Implements";
    /// Method to owning type or generic function.
    pub const METHOD_OF: &str = "MethodOf";
    /// Decorator to definition.
    pub const DECORATES: &str = "Decorates";
    /// Overriding method to base method.
    pub const OVERRIDES: &str = "Overrides";
    /// Specialisation to generic definition.
    pub const INSTANCE_OF: &str = "InstanceOf";
    /// Provenance between representation layers.
    pub const LOWERED_TO: &str = "LoweredTo";
    /// Test to tested definition.
    pub const TESTS: &str = "Tests";
    /// Module to imported module or definition.
    pub const IMPORTS: &str = "Imports";
}

impl NodeKind {
    /// Construct a definition kind without changing the open kind model.
    pub fn definition() -> Self {
        Self::Custom(code::DEFINITION.into())
    }
    /// Read both the L2 vocabulary and pre-migration symbols.
    pub fn is_definition(&self) -> bool {
        matches!(self, Self::Symbol) || matches!(self, Self::Custom(k) if k == code::DEFINITION)
    }
}
