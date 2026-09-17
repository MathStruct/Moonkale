//! Rust symbols via tree-sitter: `fn`, `struct`, `enum`, `trait`, `mod`,
//! `impl`, `const`, `static`, `type`. Items nested in `impl`/`mod` bodies are
//! found too and linked to their parent with `Contains`.
//!
//! Symbol nodes have native key `"<path>#<kind>:<name>@<line>"` so two
//! `fn new` in different `impl` blocks stay distinct and stable across
//! re-indexing as long as they don't move.

use super::Derived;
use crate::graph::derived_id;
use moonkale_core::{Edge, EdgeKind, Node, NodeId, NodeKind, SourceId};
use tree_sitter::{Node as TsNode, Parser};

const ITEM_KINDS: &[(&str, &str)] = &[
    ("function_item", "fn"),
    ("function_signature_item", "fn"),
    ("struct_item", "struct"),
    ("enum_item", "enum"),
    ("trait_item", "trait"),
    ("mod_item", "mod"),
    ("impl_item", "impl"),
    ("const_item", "const"),
    ("static_item", "static"),
    ("type_item", "type"),
    ("union_item", "union"),
    ("macro_definition", "macro"),
];

pub fn extract(index: &SourceId, file: &Node, text: &str) -> Derived {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return Derived::default();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Derived::default();
    };
    let mut d = Derived::default();
    walk(index, file, text.as_bytes(), tree.root_node(), None, &mut d);
    d
}

fn walk(
    index: &SourceId,
    file: &Node,
    src: &[u8],
    node: TsNode,
    parent: Option<NodeId>,
    d: &mut Derived,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let Some((_, short)) = ITEM_KINDS.iter().find(|(k, _)| *k == child.kind()) else {
            // Descend through declaration lists / bodies without a symbol of their own.
            if matches!(child.kind(), "declaration_list" | "source_file") {
                walk(index, file, src, child, parent, d);
            }
            continue;
        };
        let name = if *short == "impl" {
            let ty = child
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("?");
            match child
                .child_by_field_name("trait")
                .and_then(|n| n.utf8_text(src).ok())
            {
                Some(tr) => format!("{tr} for {ty}"),
                None => ty.to_string(),
            }
        } else {
            child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("?")
                .to_string()
        };
        let line = child.start_position().row + 1;
        let key = format!("{}#{}:{}@{}", file.native_key, short, name, line);
        let id = derived_id(index, &key);
        d.nodes.push(Node {
            id,
            source: index.clone(),
            kind: NodeKind::Symbol,
            label: format!("{short} {name}"),
            native_key: key,
            content: None,
            version: file.version,
        });
        d.edges.push(Edge {
            source: index.clone(),
            from: file.id,
            to: id,
            kind: EdgeKind::Defines,
        });
        if let Some(p) = parent {
            d.edges.push(Edge {
                source: index.clone(),
                from: p,
                to: id,
                kind: EdgeKind::Contains,
            });
        }
        // Items inside impl/mod/trait bodies.
        if let Some(body) = child.child_by_field_name("body") {
            walk(index, file, src, body, Some(id), d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{ContentRef, Version};

    #[test]
    fn extracts_items_and_nesting() {
        let index = SourceId::new("index:test");
        let file = Node {
            id: NodeId::derive(&SourceId::new("folder:test"), "src/lib.rs"),
            source: SourceId::new("folder:test"),
            kind: NodeKind::File,
            label: "lib.rs".into(),
            native_key: "src/lib.rs".into(),
            content: Some(ContentRef::Text {
                len: 0,
                lang: Some("rust".into()),
            }),
            version: Version(1),
        };
        let src = "pub struct A;\nimpl A {\n    pub fn new() -> Self { A }\n}\nfn free() {}\nmod m { pub fn inner() {} }\n";
        let d = extract(&index, &file, src);
        let labels: Vec<_> = d.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            ["struct A", "impl A", "fn new", "fn free", "mod m", "fn inner"]
        );
        let defines = d
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Defines)
            .count();
        let contains = d
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count();
        assert_eq!((defines, contains), (6, 2));
    }
}
