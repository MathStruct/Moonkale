//! Versioned, conservative syntactic content identities. No compiler or code execution.
//! A failed request never publishes a partial identity. See Code Graph Schema §4.

mod cache;
mod component;
mod normalize;

pub(crate) use cache::Cache;
use moonkale_core::{NodeId, Value};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Changes whenever canonical encoding or language normalization changes.
pub const NORMALIZER_VERSION: u32 = 1;
/// Maximum bytes fetched for a single source file.
pub const MAX_FILE_BYTES: usize = 512 * 1024;
/// Maximum reachable definitions in one request.
pub const MAX_DEFINITIONS: usize = 4096;
/// Maximum canonicalization work per request (syntax nodes and graph visits).
pub const MAX_STEPS: usize = 1_000_000;
/// Maximum aggregate canonical syntax bytes retained by a request.
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;

/// A content identity, distinct from a definition's source-owned location id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentIdentity {
    pub id: NodeId,
    /// Full BLAKE3 digest; the UUID alone is not the full digest.
    pub digest: [u8; 32],
    pub normalizer_version: u32,
    pub component_digest: [u8; 32],
    pub component_index: u32,
    pub component_size: u32,
}

/// A conservative failure: no approximate identity is substituted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentIdError {
    NotDefinition,
    UnsupportedSyntax,
    MalformedSyntax,
    LimitExceeded,
    StaleInput,
    SourceUnavailable,
    WorkerFailed,
}

impl std::fmt::Display for ContentIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "content identity: {self:?}")
    }
}
impl std::error::Error for ContentIdError {}

type Result<T> = std::result::Result<T, ContentIdError>;

#[derive(Clone)]
pub(crate) enum Part {
    Bytes(Vec<u8>),
    Reference(NodeId),
}

#[derive(Default)]
struct Budget(usize, usize);
impl Budget {
    fn step(&mut self) -> Result<()> {
        self.0 += 1;
        if self.0 > MAX_STEPS || self.1 > MAX_INPUT_BYTES {
            Err(ContentIdError::LimitExceeded)
        } else {
            Ok(())
        }
    }
}

/// Unambiguous length-delimited field. All integers in the encoding are LE.
fn field(out: &mut Vec<u8>, tag: &[u8], bytes: &[u8]) {
    out.extend_from_slice(&(tag.len() as u32).to_le_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

pub(crate) fn calculate(
    index: &moonkale_core::SourceId,
    file: &moonkale_core::Node,
    text: &str,
    requested: NodeId,
) -> Result<HashMap<NodeId, ContentIdentity>> {
    use crate::extract::{symbols_julia, symbols_python, symbols_rust};
    if text.len() > MAX_FILE_BYTES {
        return Err(ContentIdError::LimitExceeded);
    }
    let lang = match &file.content {
        Some(moonkale_core::ContentRef::Text { lang: Some(l), .. }) => l.as_str(),
        _ => return Err(ContentIdError::UnsupportedSyntax),
    };
    let language = match lang {
        "rust" => arborium_rust::language(),
        "python" => arborium_python::language(),
        "julia" => arborium_julia::language(),
        _ => return Err(ContentIdError::UnsupportedSyntax),
    };
    let mut parser = arborium_tree_sitter::Parser::new();
    parser
        .set_language(&language.into())
        .map_err(|_| ContentIdError::UnsupportedSyntax)?;
    // Bound parser work as well as the subsequent canonical walk.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut checks = 0usize;
    let mut progress = |_state: &arborium_tree_sitter::ParseState| {
        checks += 1;
        if checks > MAX_STEPS || std::time::Instant::now() >= deadline {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    };
    let options = arborium_tree_sitter::ParseOptions::new().progress_callback(&mut progress);
    let mut input = |offset: usize, _| &text.as_bytes()[offset.min(text.len())..];
    let tree = parser
        .parse_with_options(&mut input, None, Some(options))
        .ok_or(ContentIdError::LimitExceeded)?;
    let root = tree.root_node();
    let mut budget = Budget::default();
    // Protect the existing recursive item walkers and bound resolver input.
    let mut stack = vec![(root, 0)];
    let mut definition_count = 0;
    while let Some((node, depth)) = stack.pop() {
        budget.step()?;
        if depth > 128 {
            return Err(ContentIdError::LimitExceeded);
        }
        if matches!(
            node.kind(),
            "function_item"
                | "function_signature_item"
                | "function_definition"
                | "struct_item"
                | "enum_item"
                | "trait_item"
                | "mod_item"
                | "const_item"
                | "static_item"
                | "type_item"
                | "union_item"
                | "macro_definition"
                | "class_definition"
                | "struct_definition"
                | "module_definition"
                | "abstract_definition"
                | "primitive_definition"
                | "const_statement"
        ) || (lang == "julia"
            && node.kind() == "assignment"
            && node.named_child(0).is_some_and(|n| {
                matches!(
                    n.kind(),
                    "call_expression" | "where_expression" | "typed_expression"
                )
            }))
        {
            definition_count += 1;
            if definition_count > MAX_DEFINITIONS {
                return Err(ContentIdError::LimitExceeded);
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor).map(|child| (child, depth + 1)));
    }
    let mut symbols = crate::extract::Symbols::new(index, file, text);
    match lang {
        "rust" => symbols_rust::walk(text.as_bytes(), root, None, &mut symbols),
        "python" => symbols_python::walk(text.as_bytes(), root, None, &mut symbols),
        "julia" => symbols_julia::walk(text.as_bytes(), root, None, &mut symbols),
        _ => unreachable!(),
    }
    let derived = symbols.finish(root, text);
    let definitions: HashMap<_, _> = derived
        .nodes
        .iter()
        .filter(|n| n.kind.is_definition())
        .map(|n| (n.id, n))
        .collect();
    if !definitions.contains_key(&requested) {
        return Err(ContentIdError::NotDefinition);
    }
    let mut pending = vec![requested];
    let mut seen = HashSet::new();
    let mut units = HashMap::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if seen.len() > MAX_DEFINITIONS {
            return Err(ContentIdError::LimitExceeded);
        }
        let occurrence = derived
            .edges
            .iter()
            .find(|e| e.kind == moonkale_core::EdgeKind::Defines && e.to == id)
            .ok_or(ContentIdError::NotDefinition)?;
        let path = match occurrence.props.get("syntax_path") {
            Some(Value::Text(path)) => path,
            _ => return Err(ContentIdError::MalformedSyntax),
        };
        let mut syntax = root;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let child = part
                .parse::<u32>()
                .map_err(|_| ContentIdError::MalformedSyntax)?;
            syntax = syntax.child(child).ok_or(ContentIdError::MalformedSyntax)?;
        }
        let parts =
            normalize::definition(lang, syntax, text, id, &derived, &definitions, &mut budget)?;
        for part in &parts {
            if let Part::Reference(target) = part {
                pending.push(*target);
            }
        }
        units.insert(id, parts);
    }
    budget.step()?;
    component::identities(&units, &mut budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{ContentRef, Node, NodeKind, SourceId, Version};
    #[test]
    fn too_many_file_definitions_fail_before_reference_extraction() {
        let source = SourceId::new("folder:content-bound");
        let index = SourceId::new("index:content-bound");
        let file = Node {
            id: NodeId::derive(&source, "a.rs"),
            source,
            kind: NodeKind::File,
            label: "a.rs".into(),
            native_key: "a.rs".into(),
            props: Default::default(),
            version: Version(1),
            content: Some(ContentRef::Text {
                len: 0,
                lang: Some("rust".into()),
            }),
        };
        let text: String = (0..=MAX_DEFINITIONS)
            .map(|n| format!("fn f{n}() {{}}\n"))
            .collect();
        assert_eq!(
            calculate(&index, &file, &text, NodeId::from_content(&[0])),
            Err(ContentIdError::LimitExceeded)
        );
    }
}
