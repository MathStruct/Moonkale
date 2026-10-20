//! Versioned, conservative syntactic content identities. No compiler or code execution.
//! A failed request never publishes a partial identity. See Code Graph Schema §4.
//! The normal form and hashing are `moonkale-content-id`, shared with Sophia;
//! this module resolves references with the index's symbol extractors.

mod cache;

pub(crate) use cache::Cache;
use moonkale_content_id::{Budget, Part};
pub use moonkale_content_id::{
    ContentIdError, MAX_DEFINITIONS, MAX_FILE_BYTES, MAX_INPUT_BYTES, MAX_STEPS, NORMALIZER_VERSION,
};
use moonkale_core::{EdgeKind, NodeId, Value};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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

type Result<T> = std::result::Result<T, ContentIdError>;

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
    let mut budget = Budget::new();
    let tree = moonkale_content_id::parse(lang, text, &mut budget)?;
    let root = tree.root_node();
    let mut symbols = crate::extract::Symbols::new(index, file, text);
    match lang {
        "rust" => symbols_rust::walk(text.as_bytes(), root, None, &mut symbols),
        "python" => symbols_python::walk(text.as_bytes(), root, None, &mut symbols),
        "julia" => symbols_julia::walk(text.as_bytes(), root, None, &mut symbols),
        _ => unreachable!(),
    }
    let derived = symbols.finish(root, text);
    let definitions: HashSet<_> = derived
        .nodes
        .iter()
        .filter(|n| n.kind.is_definition())
        .map(|n| n.id)
        .collect();
    if !definitions.contains(&requested) {
        return Err(ContentIdError::NotDefinition);
    }
    let mut pending = vec![requested];
    let mut seen = HashSet::new();
    let mut units: HashMap<NodeId, Vec<Part<NodeId>>> = HashMap::new();
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
            .find(|e| e.kind == EdgeKind::Defines && e.to == id)
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
        // The extractor's reference spans from this definition to others.
        let mut references = HashMap::new();
        for edge in &derived.edges {
            if edge.from != id
                || edge.kind != EdgeKind::References
                || !definitions.contains(&edge.to)
            {
                continue;
            }
            if let Some(Value::Text(ranges)) = edge.props.get("ranges") {
                for range in ranges.split(',') {
                    if let Some((a, b)) = range
                        .split_once(':')
                        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
                    {
                        references.insert((a, b), edge.to);
                    }
                }
            }
        }
        let parts = moonkale_content_id::definition(lang, syntax, text, &references, &mut budget)?;
        for part in &parts {
            if let Part::Reference(target) = part {
                pending.push(*target);
            }
        }
        units.insert(id, parts);
    }
    budget.step()?;
    Ok(moonkale_content_id::identities(&units, &mut budget)?
        .into_iter()
        .map(|(id, c)| {
            (
                id,
                ContentIdentity {
                    id: NodeId::from_content(&c.digest),
                    digest: c.digest,
                    normalizer_version: c.normalizer_version,
                    component_digest: c.component_digest,
                    component_index: c.component_index,
                    component_size: c.component_size,
                },
            )
        })
        .collect())
}

/// One file of a Julia module, as the index read it.
pub(crate) struct JuliaFile {
    pub node: moonkale_core::Node,
    pub text: String,
    /// The module path it was included at.
    pub module: Vec<String>,
}

/// Julia (normalizer v5): every definition of a whole module, resolved per
/// module by `moonkale-content-id` (SPEC-008). Definitions are matched to
/// the index's nodes by the span of their syntax node.
pub(crate) fn calculate_julia(
    index: &moonkale_core::SourceId,
    files: &[JuliaFile],
    dynamic: &[Vec<String>],
    requested: NodeId,
) -> Result<HashMap<NodeId, ContentIdentity>> {
    use moonkale_content_id::julia;
    let mut trees = Vec::new();
    for file in files {
        trees.push(moonkale_content_id::parse(
            "julia",
            &file.text,
            &mut Budget::new(),
        )?);
    }
    let module_files: Vec<_> = files
        .iter()
        .zip(&trees)
        .map(|(f, tree)| julia::ModuleFile {
            text: &f.text,
            tree,
            module: f.module.clone(),
        })
        .collect();
    let resolved = julia::module(&module_files, dynamic)?;
    // The index's definition nodes, by (file, span).
    let mut nodes = HashMap::new();
    for (i, (file, tree)) in files.iter().zip(&trees).enumerate() {
        let root = tree.root_node();
        let mut symbols = crate::extract::Symbols::new(index, &file.node, &file.text);
        crate::extract::symbols_julia::walk(file.text.as_bytes(), root, None, &mut symbols);
        let derived = symbols.finish(root, &file.text);
        for edge in derived.edges.iter().filter(|e| e.kind == EdgeKind::Defines) {
            let Some(Value::Text(path)) = edge.props.get("syntax_path") else {
                continue;
            };
            let mut syntax = Some(root);
            for part in path.split('/').filter(|p| !p.is_empty()) {
                syntax = part
                    .parse::<u32>()
                    .ok()
                    .and_then(|c| syntax.and_then(|s| s.child(c)));
            }
            if let Some(syntax) = syntax {
                nodes.insert((i as u32, syntax.start_byte(), syntax.end_byte()), edge.to);
            }
        }
    }
    let mut out = HashMap::new();
    let mut requested_error = None;
    for definition in resolved.definitions {
        let Some(&id) = nodes.get(&definition.key) else {
            continue;
        };
        match definition.identity {
            Ok(c) => {
                out.insert(
                    id,
                    ContentIdentity {
                        id: NodeId::from_content(&c.digest),
                        digest: c.digest,
                        normalizer_version: c.normalizer_version,
                        component_digest: c.component_digest,
                        component_index: c.component_index,
                        component_size: c.component_size,
                    },
                );
            }
            Err(e) if id == requested => requested_error = Some(e),
            Err(_) => {}
        }
    }
    match requested_error {
        Some(e) => Err(e),
        None if out.contains_key(&requested) => Ok(out),
        None => Err(ContentIdError::NotDefinition),
    }
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
