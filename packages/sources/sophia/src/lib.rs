//! Sophia code stores as store-backed graph sources. See sources-sophia.md.
#[cfg(not(target_arch = "wasm32"))]
mod source;
#[cfg(not(target_arch = "wasm32"))]
pub use source::SophiaSource;

/// Open a Sophia store file named `*.sophia` (SQLite).
pub fn openers() -> Vec<moonkale_core::SourceOpener> {
    vec![moonkale_core::SourceOpener {
        id: "sophia",
        name: "Sophia store",
        shape: moonkale_core::Shape::File,
        matches: |p| p.ends_with(".sophia"),
        #[cfg(not(target_arch = "wasm32"))]
        open: Some(|p| {
            Box::pin(async move {
                Ok(std::sync::Arc::new(SophiaSource::open(p).await?)
                    as std::sync::Arc<dyn moonkale_core::Source>)
            })
        }),
        #[cfg(target_arch = "wasm32")]
        open: None,
    }]
}
