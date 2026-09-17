//! The `Source` abstraction — how the outside world becomes graph.
//!
//! A source is anything that can answer "what nodes/edges do you have?" and
//! (optionally) accept changes: a folder, a SQL database, a graph database, a
//! key/value store, a remote Moonkale server. `core` defines only the
//! **trait and the query/result types**; concrete implementations live in
//! `moonkale-sources-*` and are native-only (drivers do not compile to wasm),
//! or in `api` as server functions for the web/mobile builds.
//!
//! ```ignore
//! #[async_trait]              // or `-> impl Future` once object-safety allows
//! pub trait Source: Send + Sync {
//!     fn id(&self) -> SourceId;
//!     fn descriptor(&self) -> SourceDescriptor;    // name, kind, capabilities
//!     async fn query(&self, q: Query) -> Result<QueryResult, SourceError>;
//!     async fn fetch(&self, node: NodeId) -> Result<Content, SourceError>;
//!     async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError>;
//!     fn subscribe(&self) -> BoxStream<'static, SourceEvent>;
//! }
//! ```
//!
//! `capabilities` is a bitset: `READ, WRITE, WATCH, TEXT_QUERY(SQL|Cypher|TypeQL),
//! FULLTEXT, VECTOR, TRANSACTIONS`. Editors and the LLM layer feature-detect
//! against it instead of matching on the database brand.

pub mod descriptor;
pub mod event;
pub mod query;
pub mod transaction;
