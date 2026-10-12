//! Extractors: `(file node, text) → derived nodes + edges`.
//!
//! Each extractor is a plain function today; the `trait Extractor` with
//! `applies()`/`extract()` and extension-contributed extractors arrive with
//! the language contribution point.

pub mod symbols_julia;
pub mod symbols_python;
pub mod symbols_rust;
pub mod wikilinks;

use crate::graph::derived_id;
use moonkale_core::{Edge, EdgeKind, Node, NodeId, NodeKind, SourceId, Value};
use std::collections::HashMap;

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

/// The symbol a tree-sitter extractor found inside `parent` (a symbol, or
/// the file itself when `None`).
pub struct Symbol<'a> {
    pub parent: Option<&'a Parent>,
    /// The short kind shown in the label (`fn`, `struct`, `def`, …).
    pub kind: &'a str,
    pub name: &'a str,
    /// 1-based line of the definition.
    pub line: usize,
}

/// A symbol as the parent of nested ones.
pub struct Parent {
    pub id: NodeId,
    pub key: String,
}

/// One file's symbols: builds their nodes and edges with stable keys.
///
/// The native key is the path of the definition, not its position:
/// `"<file>#<kind>:<name>"`, nested ones `"<parent key>/<kind>:<name>"`
/// (`src/lib.rs#impl:A/fn:new`). The n-th repeat of a key in one file
/// (two `impl A` blocks, Julia methods of one function) gets `~n`.
/// Inserting or deleting lines therefore keeps every symbol's id; only
/// renaming it, moving it to another parent, or adding a same-named
/// sibling before it changes one. The line lives in the `line` property.
pub struct Symbols<'a> {
    index: &'a SourceId,
    file: &'a Node,
    seen: HashMap<String, usize>,
    pub derived: Derived,
}

impl<'a> Symbols<'a> {
    pub fn new(index: &'a SourceId, file: &'a Node) -> Self {
        Self {
            index,
            file,
            seen: HashMap::new(),
            derived: Derived::default(),
        }
    }

    /// Add one symbol; returns it as the parent of the ones nested in it.
    pub fn add(&mut self, s: Symbol) -> Parent {
        let base = match s.parent {
            Some(p) => format!("{}/{}:{}", p.key, s.kind, s.name),
            None => format!("{}#{}:{}", self.file.native_key, s.kind, s.name),
        };
        let n = self.seen.entry(base.clone()).or_insert(0);
        *n += 1;
        let key = if *n == 1 { base } else { format!("{base}~{n}") };
        let id = derived_id(self.index, &key);
        let d = &mut self.derived;
        d.nodes.push(Node {
            props: [("line".to_string(), Value::Int(s.line as i64))].into(),
            id,
            source: self.index.clone(),
            kind: NodeKind::Symbol,
            label: format!("{} {}", s.kind, s.name),
            native_key: key.clone(),
            content: None,
            version: self.file.version,
        });
        d.edges.push(Edge {
            props: Default::default(),
            source: self.index.clone(),
            from: self.file.id,
            to: id,
            kind: EdgeKind::Defines,
        });
        if let Some(p) = s.parent {
            d.edges.push(Edge {
                props: Default::default(),
                source: self.index.clone(),
                from: p.id,
                to: id,
                kind: EdgeKind::Contains,
            });
        }
        Parent { id, key }
    }
}

#[cfg(test)]
mod tests {
    use moonkale_core::{ContentRef, Node, NodeId, NodeKind, SourceId, Value, Version};

    fn file(path: &str, lang: &str) -> Node {
        Node {
            props: Default::default(),
            id: NodeId::derive(&SourceId::new("folder:test"), path),
            source: SourceId::new("folder:test"),
            kind: NodeKind::File,
            label: path.into(),
            native_key: path.into(),
            content: Some(ContentRef::Text {
                len: 0,
                lang: Some(lang.into()),
            }),
            version: Version(1),
        }
    }

    /// `(native key, id, line)` per symbol, in document order.
    fn symbols(d: &super::Derived) -> Vec<(String, NodeId, i64)> {
        d.nodes
            .iter()
            .map(|n| {
                let Some(Value::Int(line)) = n.props.get("line") else {
                    panic!("{} has no line", n.native_key)
                };
                (n.native_key.clone(), n.id, *line)
            })
            .collect()
    }

    #[test]
    fn keys_are_paths_not_positions() {
        let index = SourceId::new("index:test");
        let f = file("src/lib.rs", "rust");
        let src = "pub struct A;\nimpl A {\n    pub fn new() -> Self { A }\n}\nimpl A {\n    fn new() {}\n}\nmod m { pub fn new() {} }\n";
        let keys: Vec<_> = symbols(&super::symbols_rust::extract(&index, &f, src))
            .into_iter()
            .map(|(k, _, line)| (k, line))
            .collect();
        let expect = [
            ("src/lib.rs#struct:A", 1),
            ("src/lib.rs#impl:A", 2),
            ("src/lib.rs#impl:A/fn:new", 3),
            ("src/lib.rs#impl:A~2", 5),
            ("src/lib.rs#impl:A~2/fn:new", 6),
            ("src/lib.rs#mod:m", 8),
            ("src/lib.rs#mod:m/fn:new", 8),
        ];
        let expect: Vec<_> = expect.iter().map(|(k, l)| (k.to_string(), *l)).collect();
        assert_eq!(keys, expect);
    }

    /// The regression this scheme exists for: an id must survive an edit
    /// above the symbol (the key used to end in `@<line>`).
    #[test]
    fn ids_survive_lines_inserted_above() {
        let index = SourceId::new("index:test");
        let cases = [
            (
                "src/lib.rs",
                "rust",
                "struct A;\nimpl A { fn new() {} }\n",
                super::symbols_rust::extract as fn(&SourceId, &Node, &str) -> super::Derived,
            ),
            (
                "src/M.jl",
                "julia",
                "module M\nf(x::Int) = 1\nf(x::Float64) = 2\nend\n",
                super::symbols_julia::extract,
            ),
            (
                "a.py",
                "python",
                "class C:\n    def go(self): pass\n",
                super::symbols_python::extract,
            ),
        ];
        for (path, lang, src, extract) in cases {
            let f = file(path, lang);
            let before = symbols(&extract(&index, &f, src));
            let after = symbols(&extract(&index, &f, &format!("\n\n\n{src}")));
            assert!(!before.is_empty(), "{path}");
            assert_eq!(before.len(), after.len(), "{path}");
            for ((k0, id0, l0), (k1, id1, l1)) in before.iter().zip(&after) {
                assert_eq!((k0, id0), (k1, id1), "{path}");
                assert_eq!(l0 + 3, *l1, "{path}: the line property follows the edit");
            }
        }
    }

    #[test]
    fn julia_methods_are_numbered_in_order() {
        let index = SourceId::new("index:test");
        let f = file("src/M.jl", "julia");
        let src = "module M\nf(x::Int) = 1\nf(x::Float64) = 2\nend\n";
        let keys: Vec<_> = symbols(&super::symbols_julia::extract(&index, &f, src))
            .into_iter()
            .map(|(k, _, _)| k)
            .collect();
        assert_eq!(
            keys,
            [
                "src/M.jl#module:M",
                "src/M.jl#module:M/function:f",
                "src/M.jl#module:M/function:f~2"
            ]
        );
    }
}
