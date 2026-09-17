//! # moonkale-sources-graph
//!
//! Graph databases are the *natural* source: vertices and relations map onto
//! `core::graph` with almost no lifting. The interesting work is:
//!
//! - [`dialect`]: three query languages (Cypher for Falkor/Ladybug, TypeQL
//!   for TypeDB, HelixQL for Helix) behind one `TextQuery { dialect }`;
//! - [`structured`]: the structured `Query` IR → each language, especially
//!   neighbourhood expansion with depth/kind filters (the graph view's bread
//!   and butter);
//! - [`schema`]: TypeDB's rich type system vs. property-graph label sets,
//!   both flattened to `Schema`.
//!
//! Maturity differs a lot between these drivers (see vault
//! `research/Database Backends.md`); start with **Ladybug/Kuzu (embedded,
//! no server) and FalkorDB (Redis protocol, simple)**, then TypeDB, then Helix.

pub mod dialect;
#[cfg(feature = "falkor")]
pub mod falkor;
#[cfg(feature = "helix")]
pub mod helix;
#[cfg(feature = "ladybug")]
pub mod ladybug;
pub mod schema;
pub mod structured;
#[cfg(feature = "typedb")]
pub mod typedb;
