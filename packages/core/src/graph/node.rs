//! `Node` — the unit of everything.
//!
//! ```ignore
//! pub struct Node {
//!     pub id: NodeId,
//!     pub source: SourceId,        // who owns the truth for this node
//!     pub kind: NodeKind,          // open enum, see below
//!     pub label: String,           // what the UI shows when it has one line
//!     pub props: PropertyMap,      // typed key/value bag (graph/property.rs)
//!     pub content: Option<ContentRef>, // lazily fetched body (text, blob, rows)
//!     pub version: Version,        // monotonically increasing per source; used
//!                                  // for optimistic concurrency and undo
//! }
//! ```
//!
//! `NodeKind` is an *open* enum: a fixed set of well-known kinds that editors
//! can pattern-match on (`File`, `Directory`, `Table`, `Row`, `Vertex`,
//! `Key`, `Page`, `Symbol`, `Block`) plus `Custom(ExtensionId, String)` so an
//! extension can introduce "ModelingToolkit.jl component" without a core
//! change. Editors advertise which kinds they can open (see
//! `ext-api::contrib::EditorContribution`).
//!
//! `ContentRef` describes *how* to get the body, not the body:
//! `Text { lang: Option<LanguageId>, len: u64 }`, `Blob { mime, len }`,
//! `Rows { schema, approx_count }`, `Nested { .. }`.
