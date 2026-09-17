//! Bounded walk of a folder source through its own `Query::Children`.

use moonkale_core::{ContentRef, Node, NodeId, NodeKind, Query, Source, SourceError};

pub struct Limits {
    pub max_files: usize,
    pub max_text_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 5000,
            max_text_bytes: 512 * 1024,
        }
    }
}

/// Every node under `root`, breadth-first, with the parent of each, until
/// `max_files` files were seen. Directories named `.git`, `target` and
/// `node_modules` are skipped even when not git-ignored.
pub async fn walk(
    source: &dyn Source,
    root: NodeId,
    limits: &Limits,
) -> Result<(Vec<(NodeId, Node)>, bool), SourceError> {
    let mut out = Vec::new();
    let mut queue = vec![root];
    let mut files = 0usize;
    let mut truncated = false;
    while let Some(dir) = queue.pop() {
        let res = source.query(Query::Children(dir)).await?;
        for node in res.nodes {
            if node.kind == NodeKind::Directory {
                if matches!(node.label.as_str(), ".git" | "target" | "node_modules") {
                    continue;
                }
                queue.push(node.id);
            } else {
                files += 1;
                if files > limits.max_files {
                    truncated = true;
                    return Ok((out, truncated));
                }
            }
            out.push((dir, node));
        }
    }
    Ok((out, truncated))
}

/// Whether a file's text should be fetched for extraction.
pub fn wants_text(node: &Node, limits: &Limits) -> bool {
    match &node.content {
        Some(ContentRef::Text { len, lang }) => {
            *len <= limits.max_text_bytes
                && matches!(lang.as_deref(), Some("markdown") | Some("rust"))
        }
        _ => false,
    }
}
