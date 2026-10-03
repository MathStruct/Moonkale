// The embedded HelixDB client's futures are deeply nested (Milestone 17).
#![recursion_limit = "256"]

//! # moonkale-sources-graph
//!
//! Graph databases are the *natural* source: vertices and relations map onto
//! `core::graph` with almost no lifting. Built: LadybugDB (feature `ladybug`,
//! Cypher) and embedded HelixDB (feature `helix`, the `helix` dialect), both
//! read-only. TypeDB and FalkorDB are designed only — see the vault's
//! `architecture/Data Sources.md` ("Driver designs not built yet").

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

/// Open a LadybugDB database, if this build has LadybugDB (the `ladybug`
/// feature: on for Linux builds of the app, P-144); `None` otherwise, so
/// callers need no feature checks of their own.
pub fn open_ladybug(
    path: &std::path::Path,
) -> Option<Result<std::sync::Arc<dyn moonkale_core::Source>, moonkale_core::SourceError>> {
    #[cfg(all(feature = "ladybug", not(target_arch = "wasm32")))]
    {
        Some(
            ladybug::LadybugSource::open(path)
                .map(|s| std::sync::Arc::new(s) as std::sync::Arc<dyn moonkale_core::Source>),
        )
    }
    #[cfg(not(all(feature = "ladybug", not(target_arch = "wasm32"))))]
    {
        let _ = path;
        None
    }
}

pub fn is_ladybug_path(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .map(|e| LADYBUG_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
pub mod helix;
#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
pub use helix::HelixSource;
#[cfg(feature = "ladybug")]
pub mod ladybug;

use moonkale_core::source::opener::{OpenFuture, Shape, SourceOpener};

type Open = Option<fn(std::path::PathBuf) -> OpenFuture>;

#[cfg(all(feature = "helix", not(target_arch = "wasm32")))]
const HELIX: Open = Some(|p| {
    Box::pin(async move {
        Ok(std::sync::Arc::new(HelixSource::open(p).await?)
            as std::sync::Arc<dyn moonkale_core::Source>)
    })
});
#[cfg(not(all(feature = "helix", not(target_arch = "wasm32"))))]
const HELIX: Open = None;

#[cfg(all(feature = "ladybug", not(target_arch = "wasm32")))]
const LADYBUG: Open = Some(|p| {
    Box::pin(async move {
        Ok(std::sync::Arc::new(ladybug::LadybugSource::open(p)?)
            as std::sync::Arc<dyn moonkale_core::Source>)
    })
});
#[cfg(not(all(feature = "ladybug", not(target_arch = "wasm32"))))]
const LADYBUG: Open = None;

/// The graph source openers (Milestone 18 phase 2). LadybugDB is a file
/// (0.11 and later) or a directory (older databases).
pub fn openers() -> Vec<SourceOpener> {
    vec![
        SourceOpener {
            id: "helix",
            name: "HelixDB store",
            shape: Shape::Dir,
            matches: is_helix_path,
            open: HELIX,
        },
        SourceOpener {
            id: "ladybug",
            name: "LadybugDB database",
            shape: Shape::Either,
            matches: is_ladybug_path,
            open: LADYBUG,
        },
    ]
}
