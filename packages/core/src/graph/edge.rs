//! `Edge` — a typed, directed, optionally weighted relation between nodes.
//!
//! ```ignore
//! pub struct Edge {
//!     pub id: EdgeId,
//!     pub source: SourceId,
//!     pub from: NodeId,
//!     pub to: NodeId,
//!     pub kind: EdgeKind,          // Contains, References, Links, Calls,
//!                                  // ForeignKey, Custom(ExtensionId, String)
//!     pub props: PropertyMap,
//!     pub weight: Option<f32>,     // for layout / ranking, not semantics
//! }
//! ```
//!
//! Why edges carry a `source`: an edge can span two sources (a markdown page
//! in a folder links to a row in Postgres). The source that *stores* the edge
//! is its owner; the target may be foreign. Cross-source edges are what make
//! "mixed knowledge and code graphs" possible and are also the hardest thing
//! to keep consistent — see `markdown/problems/Problem Ranking.md`.
//!
//! Direction is semantic (`from` contains `to`), but the graph view is free to
//! draw it any way it likes (arrow colour, bidirectional style, etc.).
