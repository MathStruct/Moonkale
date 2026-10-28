//! Extractors: `(file node, text) → derived nodes + edges`.
//!
//! Each extractor is a plain function today; the `trait Extractor` with
//! `applies()`/`extract()` and extension-contributed extractors arrive with
//! the language contribution point.

mod imports;
mod references;
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
#[derive(Clone, Default, Debug)]
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
    pub syntax: arborium_tree_sitter::Node<'a>,
    pub identity: Option<String>,
}

/// A symbol as the parent of nested ones.
pub struct Parent {
    pub id: NodeId,
    pub key: String,
    pub kind: String,
}

/// One file's symbols: builds their nodes and edges with stable keys.
///
/// The native key is the path of the definition, not its position:
/// `"<file>#<kind>:<name>"`, nested ones `"<parent key>/<kind>:<name>"`
/// (`src/lib.rs#impl:A/fn:new`). The n-th repeat of a key in one file
/// (for example two `impl A` blocks) gets `~n`. Julia methods use
/// their whitespace-normalised signature before duplicate numbering.
/// Inserting or deleting lines therefore keeps every symbol's id; only
/// renaming it, moving it to another parent, or adding a same-named
/// sibling before it changes one. The line lives in the `line` property.
pub struct Symbols<'a> {
    index: &'a SourceId,
    file: &'a Node,
    seen: HashMap<String, usize>,
    pub derived: Derived,
    provenance: moonkale_core::graph::Properties,
    lang: String,
}

impl<'a> Symbols<'a> {
    pub fn new(index: &'a SourceId, file: &'a Node, text: &str) -> Self {
        Self {
            index,
            file,
            seen: HashMap::new(),
            derived: Derived::default(),
            lang: match &file.content {
                Some(moonkale_core::ContentRef::Text {
                    lang: Some(lang), ..
                }) => lang.clone(),
                _ => file.language_hint().unwrap_or("unknown").into(),
            },
            provenance: provenance(text),
        }
    }

    /// Resolve only a unique same-file name in a containing lexical scope.
    pub fn finish(self, root: arborium_tree_sitter::Node, text: &str) -> Derived {
        references::extract(
            self.derived,
            self.index,
            self.file,
            &self.provenance,
            root,
            text,
        )
    }

    /// Add one symbol; returns it as the parent of the ones nested in it.
    pub fn add(&mut self, s: Symbol) -> Parent {
        let identity = s.identity.as_deref().unwrap_or(s.name);
        let base = match s.parent {
            Some(p) => format!("{}/{}:{}", p.key, s.kind, identity),
            None => format!("{}#{}:{}", self.file.native_key, s.kind, identity),
        };
        let n = self.seen.entry(base.clone()).or_insert(0);
        *n += 1;
        let key = if *n == 1 { base } else { format!("{base}~{n}") };
        let id = derived_id(self.index, &key);
        let d = &mut self.derived;
        d.nodes.push(Node {
            props: {
                let mut props = self.provenance.clone();
                if s.kind == "mod" {
                    let supported = unattributed(s.syntax);
                    props.insert("module_path_supported".into(), Value::Bool(supported));
                    props.insert(
                        "external_module".into(),
                        Value::Bool(supported && s.syntax.child_by_field_name("body").is_none()),
                    );
                }
                props.extend([
                    ("line".into(), Value::Int(s.line as i64)),
                    ("name".into(), Value::Text(s.name.into())),
                    ("lang".into(), Value::Text(self.lang.clone())),
                    ("frontend".into(), Value::Text("moonkale-index".into())),
                    (
                        "frontend_version".into(),
                        Value::Text(env!("CARGO_PKG_VERSION").into()),
                    ),
                    (
                        "def_kind".into(),
                        Value::Text(
                            match s.kind {
                                "fn" | "def"
                                    if s.parent.is_some_and(|p| {
                                        matches!(p.kind.as_str(), "impl" | "trait" | "class")
                                    }) =>
                                {
                                    "method"
                                }
                                "fn" | "def" | "function" => "function",
                                "struct" | "enum" | "class" | "type" | "union"
                                | "primitive type" => "type",
                                "trait" | "abstract type" => "interface",
                                "mod" | "module" => "module",
                                "const" | "static" => "constant",
                                other => other,
                            }
                            .into(),
                        ),
                    ),
                ]);
                props
            },
            id,
            source: self.index.clone(),
            kind: if s.kind == "impl" {
                NodeKind::Custom("rust:Impl".into())
            } else {
                NodeKind::definition()
            },
            label: format!("{} {}", s.kind, s.name),
            native_key: key.clone(),
            content: None,
            version: self.file.version,
        });
        d.edges.push(Edge {
            props: {
                let mut props = self.provenance.clone();
                props.extend([
                    (
                        "start_byte".into(),
                        Value::Int(s.syntax.start_byte() as i64),
                    ),
                    ("end_byte".into(), Value::Int(s.syntax.end_byte() as i64)),
                    ("start_line".into(), Value::Int(s.line as i64)),
                    (
                        "end_line".into(),
                        Value::Int(s.syntax.end_position().row as i64 + 1),
                    ),
                    ("syntax_path".into(), Value::Text(syntax_path(s.syntax))),
                ]);
                props
            },
            source: self.index.clone(),
            from: self.file.id,
            to: id,
            kind: EdgeKind::Defines,
        });
        if let Some(p) = s.parent {
            d.edges.push(Edge {
                props: self.provenance.clone(),
                source: self.index.clone(),
                from: p.id,
                to: id,
                kind: EdgeKind::Contains,
            });
        }
        if s.kind == "impl" {
            return Parent {
                id,
                key,
                kind: s.kind.into(),
            };
        }
        let name_key = match s.parent {
            Some(parent) => format!("name:{}/{}", parent.key, s.name),
            None => format!("name:{}#{}", self.file.native_key, s.name),
        };
        let name_id = derived_id(self.index, &name_key);
        if !d.nodes.iter().any(|n| n.id == name_id) {
            d.nodes.push(Node {
                id: name_id,
                source: self.index.clone(),
                kind: NodeKind::Custom(moonkale_core::graph::code::NAME.into()),
                label: s.name.into(),
                native_key: name_key,
                content: None,
                version: self.file.version,
                props: self.provenance.clone(),
            });
        }
        d.edges.push(Edge {
            source: self.index.clone(),
            from: name_id,
            to: id,
            kind: EdgeKind::Custom(moonkale_core::graph::code::NAMES.into()),
            props: self.provenance.clone(),
        });
        Parent {
            id,
            key,
            kind: s.kind.into(),
        }
    }
}

// Attributes can select different module paths or condition an import.
// Comments between an attribute and its item do not remove the attribute.
fn unattributed(node: arborium_tree_sitter::Node) -> bool {
    if node.child_by_field_name("attributes").is_some() {
        return false;
    }
    let mut previous = node.prev_named_sibling();
    while let Some(n) = previous {
        if matches!(n.kind(), "line_comment" | "block_comment") {
            previous = n.prev_named_sibling();
        } else {
            return n.kind() != "attribute_item";
        }
    }
    true
}

/// Input identity is a domain-separated UUID-v5 digest of exact file bytes.
/// It is independent of a source's potentially mtime-based `Version`.
pub fn provenance(text: &str) -> moonkale_core::graph::Properties {
    [
        ("by".into(), Value::Text("moonkale-index".into())),
        (
            "by_version".into(),
            Value::Text(env!("CARGO_PKG_VERSION").into()),
        ),
        (
            "input".into(),
            Value::Text(NodeId::derive(&SourceId::new("index-input-v1"), text).to_string()),
        ),
        (
            "derived_at".into(),
            Value::Int(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            ),
        ),
    ]
    .into()
}

/// Child indices in the actual tree (including unnamed punctuation nodes).
fn syntax_path(mut node: arborium_tree_sitter::Node) -> String {
    let mut path = Vec::new();
    while let Some(parent) = node.parent() {
        let mut cursor = parent.walk();
        if let Some(index) = parent
            .children(&mut cursor)
            .position(|child| child.id() == node.id())
        {
            path.push(index.to_string());
        }
        node = parent;
    }
    path.reverse();
    path.join("/")
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
            .filter(|n| {
                n.kind == NodeKind::definition()
                    || matches!(&n.kind, NodeKind::Custom(k) if k == "rust:Impl")
            })
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
    fn julia_methods_use_signatures() {
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
                "src/M.jl#module:M/function:f(x::Int)",
                "src/M.jl#module:M/function:f(x::Float64)"
            ]
        );
    }
    #[test]
    fn julia_method_ids_survive_reordering_and_body_changes() {
        let index = SourceId::new("index:test");
        let f = file("a.jl", "julia");
        let before = super::symbols_julia::extract(&index, &f, "f(x::Int) = 1\nf(x::String) = 2\n");
        let after =
            super::symbols_julia::extract(&index, &f, "f(x::String) = 42\nf(x::Int) = 77\n");
        let mut before = symbols(&before)
            .into_iter()
            .map(|(key, id, _)| (key, id))
            .collect::<Vec<_>>();
        let mut after = symbols(&after)
            .into_iter()
            .map(|(key, id, _)| (key, id))
            .collect::<Vec<_>>();
        before.sort();
        after.sort();
        assert_eq!(before, after);
    }
}
