//! Shared local KaTeX assets for Rich mode and native source-backed preview.
use dioxus::prelude::*;
/// KaTeX stylesheet and woff2 fonts, retaining relative font URLs.
pub const KATEX_DIR: Asset = asset!("/assets/katex", AssetOptions::folder());
/// Isolated TeX presentation renderer; Markdown parsing stays in Rust.
pub const KATEX_RENDERER: Asset = asset!("/assets/katex-renderer.js");
