//! Python symbols via tree-sitter: `def` and `class` (decorated or not),
//! methods inside classes linked with `Contains`. Module-level assignments
//! are not symbols (too many, too noisy).

use super::Derived;
use crate::graph::derived_id;
use arborium_tree_sitter::{Node as TsNode, Parser};
use moonkale_core::{Edge, EdgeKind, Node, NodeId, NodeKind, SourceId};

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
                    walk(index, file, src, def, parent, d);
                }
                continue;
            }
        };
        let name = def
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(src).ok())
            .unwrap_or("?");
        let line = def.start_position().row + 1;
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
        // Methods and nested classes; functions inside functions are not symbols.
        if short == "class" {
            if let Some(body) = def.child_by_field_name("body") {
                walk(index, file, src, body, Some(id), d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{ContentRef, Version};

    #[test]
    fn extracts_defs_and_classes() {
        let index = SourceId::new("index:test");
        let file = Node {
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
