//! Lifting: relational / graph / KV → `core::graph`.
//!
//! - **SQL**: table → `NodeKind::Table`; row → `NodeKind::Row` with id
//!   derived from the primary key; foreign key → `EdgeKind::ForeignKey`;
//!   schema → nodes of kind `Table`/`Column` linked by `Contains`.
//!   Tables without a PK get row ids from `ctid`/`rowid`/hash — flagged
//!   unstable in the descriptor.
//! - **Graph DBs**: vertex → `Vertex`, relation → edge, 1:1. TypeDB's typed
//!   attributes/roles map to properties + `Custom` edge kinds.
//! - **KV**: key → `Key` node; a configurable *key pattern* (`user:{id}:*`)
//!   groups keys into synthetic parent nodes so a keyspace isn't a flat
//!   million-node list.
//!
//! Lifting is lazy and query-driven; nothing is materialised until a view
//! asks for it.
