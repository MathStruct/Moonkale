//! Local Unison codebases as read-only store-backed graphs. See sources-unison.md.
#[cfg(not(target_arch = "wasm32"))]
mod source;
#[cfg(not(target_arch = "wasm32"))]
pub use source::UnisonSource;

/// Open a codebase root named `*.unison`. The directory contains `.unison/v2`.
/// Explicit `UnisonSource::open` also accepts ordinary directory names.
pub fn openers() -> Vec<moonkale_core::SourceOpener> {
    vec![moonkale_core::SourceOpener {
        id: "unison",
        name: "Unison codebase",
        shape: moonkale_core::Shape::Dir,
        matches: |p| p.trim_end_matches('/').ends_with(".unison"),
        #[cfg(not(target_arch = "wasm32"))]
        open: Some(|p| {
            Box::pin(async move {
                Ok(std::sync::Arc::new(UnisonSource::open(p).await?)
                    as std::sync::Arc<dyn moonkale_core::Source>)
            })
        }),
        #[cfg(target_arch = "wasm32")]
        open: None,
    }]
}
