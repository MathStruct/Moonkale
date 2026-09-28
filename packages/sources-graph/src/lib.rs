// The embedded HelixDB client's futures are deeply nested (Milestone 17).
#![recursion_limit = "256"]

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

/// Names that open as a LadybugDB database (a directory for databases
/// created before 0.11, a single file since). Usable on every target; the
/// driver itself is native-only behind the `ladybug` feature.
pub const LADYBUG_EXTENSIONS: &[&str] = &["lbug", "kuzu", "kz"];

/// A HelixDB store (Milestone 17): an object-store root directory named
/// `*.helix`. Path check only, usable on every target.
pub fn is_helix_path(path: &str) -> bool {
    path.trim_end_matches('/')
        .rsplit_once('.')
        .is_some_and(|(stem, e)| {
            !stem.is_empty() && !stem.ends_with('/') && e.eq_ignore_ascii_case("helix")
        })
}

pub fn is_ladybug_path(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| LADYBUG_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
#[cfg(feature = "falkor")]
pub mod falkor;
#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
pub mod helix;
#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
pub use helix::HelixSource;
#[cfg(feature = "ladybug")]
pub mod ladybug;
pub mod schema;
pub mod structured;
#[cfg(feature = "typedb")]
pub mod typedb;
