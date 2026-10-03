//! Extractors: `(file node, text) → derived nodes + edges`.
//!
//! Each extractor is a plain function today; the `trait Extractor` with
//! `applies()`/`extract()` and extension-contributed extractors arrive with
//! the language contribution point.

pub mod symbols_julia;
pub mod symbols_python;
pub mod symbols_rust;
pub mod wikilinks;

use moonkale_core::{Edge, Node};

/// The languages some extractor reads. The walk fetches a file's text only
/// for these, and `IndexSource::extract_into` dispatches on the same names —
/// a language added to the dispatch but not here never runs (issue #16: the
/// Julia and Python extractors existed for weeks without being called).
pub const LANGUAGES: &[&str] = &["markdown", "rust", "julia", "python"];

/// Whether an extractor reads files of this language.
pub fn reads(lang: Option<&str>) -> bool {
    lang.is_some_and(|l| LANGUAGES.contains(&l))
}

/// What an extractor produced for one file.
#[derive(Default, Debug)]
pub struct Derived {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Derived {
    pub fn merge(&mut self, other: Derived) {
        self.nodes.extend(other.nodes);
        self.edges.extend(other.edges);
    }
}
