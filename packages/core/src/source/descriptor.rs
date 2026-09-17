//! `SourceDescriptor` — the static shape of a source.
//!
//! ```ignore
//! pub struct SourceDescriptor {
//!     pub id: SourceId,
//!     pub display_name: String,
//!     pub family: SourceFamily,     // Folder | Sql | Graph | KeyValue | Remote | Custom
//!     pub dialect: Option<Dialect>, // Postgres | Sqlite | DuckDb | Turso | TypeQl | Cypher | ...
//!     pub capabilities: Capabilities,
//!     pub schema: Option<Schema>,   // tables/columns, node/edge types, key patterns
//! }
//! ```
//!
//! `Schema` is itself expressed as a tiny graph (types are nodes, "has column"
//! / "has property" are edges), which means the schema of a database can be
//! shown in the same graph view as its data. That is not a gimmick — it is
//! exactly the picture people draw on whiteboards.
//!
//! Connection *secrets* are NOT in the descriptor. They live in the platform
//! keychain (desktop), server env (web) or secure storage (mobile); see
//! `moonkale-sources::credentials`.
