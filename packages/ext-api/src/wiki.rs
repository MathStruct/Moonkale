//! `[[wiki-links]]` as Obsidian users expect them (spec 012): completion of
//! page names, resolved/unresolved status for decorations, follow-or-create
//! on click, and link rewriting when a note is renamed. The index extracts
//! and resolves links on its own (`moonkale-index::extract::wikilinks`);
//! this module is the *editor-facing* half, built on the index's page list
//! so it works on every platform (through `RemoteSource` on the web).

use crate::fuzzy_score;
use crate::workspace::Workspace;
use dioxus::prelude::*;
use moonkale_core::{Direction, EdgeKind, Node, NodeId, NodeKind, Query, SourceError};

/// A page offered by `[[` completion.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct WikiCandidate {
    /// What goes between the brackets (the stem; a path when the stem is
    /// ambiguous among the open folders).
    pub target: String,
    /// The file's relative key, for the detail column.
    pub key: String,
}

/// One `[[target#heading|alias]]` as written in a text: the byte range of
/// the whole link and its parts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct WikiSpan {
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    pub resolved: bool,
}

/// The stem of a relative key (`notes/Beta.md` → `Beta`), the index's rule.
pub fn stem_of(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// `(start, end, target, heading, alias)` of one link as written.
pub type RawSpan = (usize, usize, String, Option<String>, Option<String>);

/// Parse `[[…]]` links in `text` (no resolution).
pub fn spans(text: &str) -> Vec<RawSpan> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // An embed `![[…]]` is a link too (rendered later, spec 012 §7).
            if let Some(close) = text[i + 2..].find("]]") {
                let inner = &text[i + 2..i + 2 + close];
                if !inner.is_empty() && !inner.contains("[[") && !inner.contains('\n') {
                    let (rest, alias) = match inner.split_once('|') {
                        Some((a, b)) => (a, Some(b.trim().to_string())),
                        None => (inner, None),
                    };
                    let (target, heading) = match rest.split_once('#') {
                        Some((a, b)) => (a, Some(b.trim().to_string())),
                        None => (rest, None),
                    };
                    let target = target.trim().to_string();
                    if !target.is_empty() {
                        out.push((i, i + 2 + close + 2, target, heading, alias));
                    }
                    i += 2 + close + 2;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

impl Workspace {
    /// Every page (file node) the index knows, across the open folders.
    async fn index_pages(&self) -> Vec<Node> {
        let Some(index) = self.index() else {
            return Vec::new();
        };
        match index
            .source
            .query(Query::All {
                limit: 20_000,
                kinds: Some(vec![NodeKind::File]),
            })
            .await
        {
            Ok(r) => r.nodes,
            Err(_) => Vec::new(),
        }
    }

    /// Resolve `target` (as written in `from`) to a page: path-like targets
    /// relative to the linking file, then from the root; otherwise by stem,
    /// case-insensitively — the index's rules, over its page list.
    pub async fn resolve_wiki(&self, from: &Node, target: &str) -> Option<Node> {
        let pages = self.index_pages().await;
        resolve_in(&pages, &from.native_key, target)
    }

    /// The `[[…]]` links in `text` with their resolution, for decorations.
    pub async fn wiki_spans(&self, from: &Node, text: &str) -> Vec<WikiSpan> {
        let pages = self.index_pages().await;
        spans(text)
            .into_iter()
            .map(|(start, end, target, heading, alias)| WikiSpan {
                resolved: resolve_in(&pages, &from.native_key, &target).is_some(),
                start,
                end,
                target,
                heading,
                alias,
            })
            .collect()
    }

    /// Pages for `[[` completion, fuzzy-ranked by `query` (empty = the most
    /// recently opened documents first, then everything alphabetically).
    pub async fn wiki_candidates(&self, query: &str, limit: usize) -> Vec<WikiCandidate> {
        let pages = self.index_pages().await;
        let mut stems: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for p in &pages {
            *stems
                .entry(stem_of(&p.native_key).to_lowercase())
                .or_default() += 1;
        }
        let target_of = |p: &Node| {
            let stem = stem_of(&p.native_key);
            if stems.get(&stem.to_lowercase()).copied().unwrap_or(0) > 1 {
                p.native_key
                    .strip_suffix(".md")
                    .unwrap_or(&p.native_key)
                    .to_string()
            } else {
                stem.to_string()
            }
        };
        let query = query.trim();
        let mut scored: Vec<(i32, WikiCandidate)> = pages
            .iter()
            .filter(|p| p.native_key.ends_with(".md"))
            .filter_map(|p| {
                let target = target_of(p);
                let score = if query.is_empty() {
                    Some(0)
                } else {
                    fuzzy_score(query, &target).or_else(|| fuzzy_score(query, &p.native_key))
                };
                score.map(|s| {
                    (
                        s,
                        WikiCandidate {
                            target,
                            key: p.native_key.clone(),
                        },
                    )
                })
            })
            .collect();
        if query.is_empty() {
            // Open documents first, most recently activated at the top.
            let open: Vec<String> = self
                .documents
                .peek()
                .iter()
                .rev()
                .map(|(_, d)| d.peek().node.native_key.clone())
                .collect();
            scored.sort_by_key(|(_, c)| {
                (
                    open.iter().position(|k| *k == c.key).unwrap_or(usize::MAX),
                    c.key.to_lowercase(),
                )
            });
        } else {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.key.cmp(&b.1.key)));
        }
        scored.into_iter().map(|(_, c)| c).take(limit).collect()
    }

    /// Follow a link written in `from`: open the page, or — when it does not
    /// resolve and `create` is set — create `Target.md` next to `from`
    /// (path-like targets under the folder root) and open it.
    pub async fn follow_wiki(
        mut self,
        from: &Node,
        target: &str,
        create: bool,
    ) -> Result<Node, SourceError> {
        if let Some(page) = self.resolve_wiki(from, target).await {
            let node = self.open_relative_path(&page.native_key).await?;
            return Ok(node);
        }
        if !create {
            self.set_status(format!("[[{target}]] does not resolve to a page"));
            return Err(SourceError::NotFound);
        }
        let rel = if target.contains('/') {
            format!("{}.md", target.trim_start_matches('/'))
        } else {
            match from.native_key.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/{target}.md"),
                None => format!("{target}.md"),
            }
        };
        let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel.as_str()));
        let (dir, name) = (dir.to_string(), name.to_string());
        let source_id = from.source.clone();
        let parent = if dir.is_empty() {
            self.source_handle(&source_id)
                .map(|s| s.descriptor.root)
                .ok_or(SourceError::NotFound)?
        } else {
            self.node_at_path(&source_id, &dir)
                .await
                .map(|n| n.id)
                .ok_or_else(|| SourceError::Unsupported(format!("{dir}: no such folder")))?
        };
        let title = stem_of(&name).to_string();
        let node = self
            .create_text(&source_id, parent, &name, &format!("# {title}\n\n"))
            .await?;
        self.open_node(node.clone()).await?;
        self.set_status(format!("Created {rel}"));
        Ok(node)
    }

    /// The files the index says link to `node` (asked *before* a rename:
    /// afterwards the old node is gone and its links have become phantoms).
    pub async fn wiki_backlinks(&self, node: NodeId) -> Vec<Node> {
        let Some(index) = self.index() else {
            return Vec::new();
        };
        let Ok(res) = index
            .source
            .query(Query::Neighbours {
                node,
                depth: 1,
                direction: Direction::In,
            })
            .await
        else {
            return Vec::new();
        };
        res.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Links && e.to == node)
            .filter_map(|e| res.nodes.iter().find(|n| n.id == e.from).cloned())
            .filter(|n| n.kind == NodeKind::File)
            .collect()
    }

    /// After renaming a markdown file from `old_key` to `new_key`: rewrite
    /// `[[old]]` → `[[new]]` in `linking` (from [`Self::wiki_backlinks`]).
    /// Stem links become the new stem; path links the new path. Returns the
    /// number of files changed.
    pub async fn rewrite_wiki_links(
        mut self,
        linking: Vec<Node>,
        old_key: &str,
        new_key: &str,
    ) -> usize {
        if !old_key.ends_with(".md") || !new_key.ends_with(".md") || linking.is_empty() {
            return 0;
        }
        let old_stem = stem_of(old_key).to_string();
        let new_stem = stem_of(new_key).to_string();
        let old_path = old_key.strip_suffix(".md").unwrap_or(old_key).to_string();
        let new_path = new_key.strip_suffix(".md").unwrap_or(new_key).to_string();
        let mut files = 0;
        for file in linking {
            // Index file nodes are the folder's own (same id and source).
            let mut changed = 0;
            for (needle, replacement) in [
                (format!("[[{old_path}]]"), format!("[[{new_path}]]")),
                (format!("[[{old_path}|"), format!("[[{new_path}|")),
                (format!("[[{old_path}#"), format!("[[{new_path}#")),
                (format!("[[{old_stem}]]"), format!("[[{new_stem}]]")),
                (format!("[[{old_stem}|"), format!("[[{new_stem}|")),
                (format!("[[{old_stem}#"), format!("[[{new_stem}#")),
            ] {
                if needle == replacement {
                    continue;
                }
                if let Ok(n) = self.replace_in_file(&file, &needle, &replacement).await {
                    changed += n;
                }
            }
            if changed > 0 {
                files += 1;
            }
        }
        if files > 0 {
            self.set_status(format!(
                "Renamed {old_stem} → {new_stem}: links updated in {files} file(s)"
            ));
        }
        files
    }

    fn source_handle(
        &self,
        id: &moonkale_core::SourceId,
    ) -> Option<crate::workspace::SourceHandle> {
        self.sources
            .peek()
            .iter()
            .find(|s| &s.descriptor.id == id)
            .cloned()
    }
}

fn resolve_in(pages: &[Node], from_rel: &str, target: &str) -> Option<Node> {
    let t = target.trim().trim_start_matches("./");
    if t.is_empty() {
        return None;
    }
    let by_path = |p: &str| {
        let p = p.trim_start_matches('/');
        pages
            .iter()
            .find(|n| n.native_key == p || n.native_key.eq_ignore_ascii_case(p))
            .cloned()
    };
    if t.contains('/') || t.ends_with(".md") {
        let dir = from_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let joined = normalize(&format!("{dir}/{t}"));
        for c in [
            joined.clone(),
            format!("{joined}.md"),
            t.to_string(),
            format!("{t}.md"),
        ] {
            if let Some(n) = by_path(&c) {
                return Some(n);
            }
        }
    }
    let stem = stem_of(t).to_lowercase();
    // Prefer a page in the same directory, then the shortest path (Obsidian).
    let dir = from_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut hits: Vec<&Node> = pages
        .iter()
        .filter(|n| stem_of(&n.native_key).to_lowercase() == stem)
        .collect();
    hits.sort_by_key(|n| {
        let same_dir = n.native_key.rsplit_once('/').map(|(d, _)| d).unwrap_or("") == dir;
        (!same_dir, n.native_key.len())
    });
    hits.first().map(|n| (*n).clone())
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

#[cfg(test)]
mod tests {
    use super::*;
    use moonkale_core::{SourceId, Version};

    fn page(key: &str) -> Node {
        let src = SourceId("index:test".into());
        Node {
            id: NodeId::derive(&src, key),
            source: src,
            kind: NodeKind::File,
            label: key.rsplit('/').next().unwrap().into(),
            native_key: key.into(),
            content: None,
            version: Version::default(),
        }
    }

    #[test]
    fn spans_parse_target_heading_alias() {
        let s =
            spans("see [[Alpha]] and [[notes/Beta#Intro|the intro]] and ![[img.png]] not [[ ]]");
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].0, s[0].1, s[0].2.as_str()), (4, 13, "Alpha"));
        assert_eq!(s[1].2, "notes/Beta");
        assert_eq!(s[1].3.as_deref(), Some("Intro"));
        assert_eq!(s[1].4.as_deref(), Some("the intro"));
        assert_eq!(s[2].2, "img.png");
    }

    #[test]
    fn resolve_by_stem_path_and_same_dir() {
        let pages = vec![
            page("Home.md"),
            page("notes/Beta.md"),
            page("a/Notes.md"),
            page("b/Notes.md"),
        ];
        assert_eq!(
            resolve_in(&pages, "Home.md", "beta").unwrap().native_key,
            "notes/Beta.md"
        );
        assert_eq!(
            resolve_in(&pages, "notes/Beta.md", "../Home")
                .unwrap()
                .native_key,
            "Home.md"
        );
        assert_eq!(
            resolve_in(&pages, "b/x.md", "Notes").unwrap().native_key,
            "b/Notes.md"
        );
        assert_eq!(
            resolve_in(&pages, "Home.md", "Notes").unwrap().native_key,
            "a/Notes.md"
        );
        assert!(resolve_in(&pages, "Home.md", "Missing").is_none());
    }
}
