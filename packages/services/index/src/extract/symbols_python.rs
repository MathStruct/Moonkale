//! Python symbols via tree-sitter: `def` and `class` (decorated or not),
//! methods inside classes linked with `Contains`. Module-level assignments
//! are not symbols (too many, too noisy).

use super::{Derived, Parent, Symbol, Symbols};
use arborium_tree_sitter::{Node as TsNode, Parser};
use moonkale_core::{Node, SourceId};

pub fn extract(index: &SourceId, file: &Node, text: &str) -> Derived {
    let mut parser = Parser::new();
    if parser
        .set_language(&arborium_python::language().into())
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
        // `@decorator` wraps the definition.
        let def = if child.kind() == "decorated_definition" {
            match child.child_by_field_name("definition") {
                Some(inner) => inner,
                None => continue,
            }
        } else {
            child
        };
        let short = match def.kind() {
            "function_definition" => "def",
            "class_definition" => "class",
            _ => {
                if matches!(def.kind(), "module" | "block") {
                    walk(src, def, parent, symbols);
                }
                continue;
            }
        };
        let name = def
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(src).ok())
            .unwrap_or("?");
        let me = symbols.add(Symbol {
            parent,
            kind: short,
            name,
            line: def.start_position().row + 1,
        });
        // Methods and nested classes; functions inside functions are not symbols.
        if short == "class" {
            if let Some(body) = def.child_by_field_name("body") {
                walk(src, body, Some(&me), symbols);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{ContentRef, EdgeKind, NodeId, NodeKind, Version};

    #[test]
    fn extracts_defs_and_classes() {
        let index = SourceId::new("index:test");
        let file = Node {
            props: Default::default(),
            id: NodeId::derive(&SourceId::new("folder:test"), "a.py"),
            source: SourceId::new("folder:test"),
            kind: NodeKind::File,
            label: "a.py".into(),
            native_key: "a.py".into(),
            content: Some(ContentRef::Text {
                len: 0,
                lang: Some("python".into()),
            }),
            version: Version(1),
        };
        let src = "import os\n\ndef free(x):\n    def inner(): pass\n    return x\n\n@dataclass\nclass A:\n    x: int = 0\n    def method(self):\n        return self.x\n    @property\n    def p(self):\n        return 1\n";
        let d = extract(&index, &file, src);
        let labels: Vec<_> = d.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["def free", "class A", "def method", "def p"]);
        let contains = d
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count();
        assert_eq!(contains, 2);
    }
}
