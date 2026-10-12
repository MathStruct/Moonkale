//! Rust symbols via tree-sitter: `fn`, `struct`, `enum`, `trait`, `mod`,
//! `impl`, `const`, `static`, `type`. Items nested in `impl`/`mod` bodies are
//! found too and linked to their parent with `Contains`.
//!
//! Keys and ids come from [`Symbols`]: `src/lib.rs#impl:A/fn:new`, so two
//! `fn new` in different `impl` blocks stay distinct and keep their ids
//! when lines move.

use super::{Derived, Parent, Symbol, Symbols};
use arborium_tree_sitter::{Node as TsNode, Parser};
use moonkale_core::{Node, SourceId};

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
        .set_language(&arborium_rust::language().into())
        .is_err()
    {
        return Derived::default();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Derived::default();
    };
    let mut symbols = Symbols::new(index, file);
    walk(text.as_bytes(), tree.root_node(), None, &mut symbols);
    symbols.derived
}

fn walk(src: &[u8], node: TsNode, parent: Option<&Parent>, symbols: &mut Symbols) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let Some((_, short)) = ITEM_KINDS.iter().find(|(k, _)| *k == child.kind()) else {
            // Descend through declaration lists / bodies without a symbol of their own.
            if matches!(child.kind(), "declaration_list" | "source_file") {
                walk(src, child, parent, symbols);
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
        let me = symbols.add(Symbol {
            parent,
            kind: short,
            name: &name,
            line: child.start_position().row + 1,
        });
        // Items inside impl/mod/trait bodies.
        if let Some(body) = child.child_by_field_name("body") {
            walk(src, body, Some(&me), symbols);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{ContentRef, EdgeKind, NodeId, NodeKind, Version};

    #[test]
    fn extracts_items_and_nesting() {
        let index = SourceId::new("index:test");
        let file = Node {
            props: Default::default(),
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
