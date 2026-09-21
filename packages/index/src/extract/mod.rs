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
