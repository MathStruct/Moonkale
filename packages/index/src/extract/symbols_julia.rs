//! Julia symbols via tree-sitter: `function`, short-form `f(x) = …`,
//! `struct`/`mutable struct`, `abstract type`, `primitive type`, `module`,
//! `macro`, `const`. Items inside `module` bodies link to the module with
//! `Contains`; nothing inside function bodies is a symbol.
//!
//! The grammar (tree-sitter-julia 0.23) has almost no named fields, so the
//! name is the head of the first named child: `signature → call_expression
//! → identifier`, through `where`/`::`/`{…}` wrappers; `Base.show` keeps
//! its dotted form.

use super::Derived;
use crate::graph::derived_id;
use arborium_tree_sitter::{Node as TsNode, Parser};
use moonkale_core::{Edge, EdgeKind, Node, NodeId, NodeKind, SourceId};

const ITEM_KINDS: &[(&str, &str)] = &[
    ("function_definition", "function"),
    ("struct_definition", "struct"),
    ("abstract_definition", "abstract type"),
    ("primitive_definition", "primitive type"),
    ("module_definition", "module"),
    ("macro_definition", "macro"),
    ("const_statement", "const"),
];

pub fn extract(index: &SourceId, file: &Node, text: &str) -> Derived {
    let mut parser = Parser::new();
    if parser
        .set_language(&arborium_julia::language().into())
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

/// The identifier a definition head boils down to.
fn head_name<'a>(node: TsNode<'a>, src: &'a [u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "field_expression" | "operator" | "macro_identifier" => {
            node.utf8_text(src).ok().map(str::to_string)
        }
        // `f(x)`, `f(x) where T`, `f(x)::T`, `Foo{T}`, `Foo <: Bar`
        "signature"
        | "call_expression"
        | "where_expression"
        | "typed_expression"
        | "parametrized_type_expression"
        | "type_head"
        | "binary_expression"
        | "assignment"
        | "parenthesized_expression" => {
            let mut c = node.walk();
            let first = node.named_children(&mut c).next();
            first.and_then(|n| head_name(n, src))
        }
        _ => None,
    }
}

/// `f(x) = …` — an assignment whose left side is a call.
fn short_function(node: TsNode, src: &[u8]) -> Option<String> {
    if node.kind() != "assignment" {
        return None;
    }
    let mut c = node.walk();
    let left = node.named_children(&mut c).next();
    let left = left?;
    if matches!(
        left.kind(),
        "call_expression" | "where_expression" | "typed_expression"
    ) {
        head_name(left, src)
    } else {
        None
    }
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
        let (short, name) = match ITEM_KINDS.iter().find(|(k, _)| *k == child.kind()) {
            Some((_, short)) => {
                let name = if *short == "module" {
                    child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(src).ok())
                        .map(str::to_string)
                } else {
                    let mut c = child.walk();
                    let found = child.named_children(&mut c).find_map(|n| head_name(n, src));
                    found
                };
                (*short, name.unwrap_or_else(|| "?".into()))
            }
            None => match short_function(child, src) {
                Some(name) => ("function", name),
                None => {
                    if matches!(child.kind(), "source_file" | "block" | "begin_statement") {
                        walk(index, file, src, child, parent, d);
                    }
                    continue;
                }
            },
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
        // Definitions inside a module.
        if short == "module" {
            walk(index, file, src, child, Some(id), d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{ContentRef, Version};

    #[test]
    fn extracts_julia_definitions() {
        let index = SourceId::new("index:test");
        let file = Node {
            id: NodeId::derive(&SourceId::new("folder:test"), "src/M.jl"),
            source: SourceId::new("folder:test"),
            kind: NodeKind::File,
            label: "M.jl".into(),
            native_key: "src/M.jl".into(),
            content: Some(ContentRef::Text {
                len: 0,
                lang: Some("julia".into()),
            }),
            version: Version(1),
        };
        let src = r#"module M
abstract type Shape end
struct Circle{T} <: Shape
    r::T
end
mutable struct Box
    w::Float64
end
const TAU = 6.28
function area(c::Circle{T}) where T
    TAU / 2 * c.r^2
end
area(b::Box) = b.w^2
Base.show(io::IO, c::Circle) = print(io, "circle")
macro twice(ex)
    :($ex; $ex)
end
end
"#;
        let d = extract(&index, &file, src);
        let labels: Vec<_> = d.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "module M",
                "abstract type Shape",
                "struct Circle",
                "struct Box",
                "const TAU",
                "function area",
                "function area",
                "function Base.show",
                "macro twice",
            ]
        );
        let contains = d
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count();
        assert_eq!(contains, 8, "everything is inside module M");
    }
}
