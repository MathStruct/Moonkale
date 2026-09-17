//! `[[wiki-links]]` and `[text](relative.md)` links between files.
//!
//! Resolution follows Obsidian: `[[Name]]` matches a file whose stem is
//! `Name` (case-insensitive), shortest path wins; `[[Name#heading]]` and
//! `[[Name|alias]]` strip the suffix; `[[folder/Name]]` matches the path.
//! Unresolved targets become **phantom `Page` nodes** owned by the index, so
//! the graph shows what a vault refers to but does not yet contain.

use super::Derived;
use crate::graph::{derived_id, IndexGraph};
use moonkale_core::{Edge, EdgeKind, Node, NodeId, NodeKind, SourceId, Version};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

fn wiki_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[([^\]\[|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]").unwrap())
}

fn md_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]*\]\(([^)\s#]+\.md)(?:#[^)]*)?\)").unwrap())
}

/// Targets referenced by `text`, as written (deduplicated, in order).
pub fn targets(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for cap in wiki_re().captures_iter(text) {
        let t = cap[1].trim().to_string();
        if !t.is_empty() && seen.insert(t.clone()) {
            out.push(t);
        }
    }
    for cap in md_re().captures_iter(text) {
        let t = cap[1].trim().to_string();
        if !t.is_empty() && seen.insert(t.clone()) {
            out.push(t);
        }
    }
    out
}

/// Resolve `target` (as written) from `from_rel` against the graph.
pub fn resolve(graph: &IndexGraph, from_rel: &str, target: &str) -> Option<NodeId> {
    let t = target.trim_start_matches("./");
    // Path-like: try relative to the file's directory, then from the root.
    if t.contains('/') || t.ends_with(".md") {
        let dir = from_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let joined = normalize(&format!("{dir}/{t}"));
        for candidate in [
            joined.clone(),
            format!("{joined}.md"),
            t.to_string(),
            format!("{t}.md"),
        ] {
            if let Some(id) = graph.file_by_path(candidate.trim_start_matches('/')) {
                return Some(id);
            }
        }
    }
    graph.file_by_stem(crate::graph::stem_of(t))
}

fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub fn extract(graph: &IndexGraph, index: &SourceId, file: &Node, text: &str) -> Derived {
    let mut d = Derived::default();
    for target in targets(text) {
        let to = match resolve(graph, &file.native_key, &target) {
            Some(id) => id,
            None => {
                // Phantom page for an unresolved link.
                let key = format!("unresolved:{}", target.to_ascii_lowercase());
                let id = derived_id(index, &key);
                if !d.nodes.iter().any(|n| n.id == id) {
                    d.nodes.push(Node {
                        id,
                        source: index.clone(),
                        kind: NodeKind::Page,
                        label: target.clone(),
                        native_key: key,
                        content: None,
                        version: Version::default(),
                    });
                }
                id
            }
        };
        if to != file.id {
            d.edges.push(Edge {
                source: index.clone(),
                from: file.id,
                to,
                kind: EdgeKind::Links,
            });
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_wiki_and_markdown_links() {
        let t = targets(
            "See [[Alpha]] and [[docs/Beta#top|B]] plus [gamma](../gamma.md) and [[Alpha]] again.",
        );
        assert_eq!(t, vec!["Alpha", "docs/Beta", "../gamma.md"]);
    }
}
