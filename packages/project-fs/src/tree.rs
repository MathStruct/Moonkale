//! Directory listing with `.gitignore` awareness.
//!
//! One level at a time: the explorer expands lazily, so a 100k-file monorepo
//! costs one `read_dir` per expanded folder, not a full walk at open time.

use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Path relative to the folder root, `/`-separated, no leading slash.
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
    pub len: u64,
}

/// List the direct children of `dir` (absolute) under `root`, honouring
/// `.gitignore`/`.ignore` files and skipping hidden entries except the ones
/// git itself would show. Sorted directories-first, then by name.
pub fn list_children(root: &Path, dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    // `ignore` needs the walk to start at `root` to pick up parent .gitignore
    // files, but we only want one level below `dir`: max_depth is relative to
    // the walk root, so compute the depth of `dir` and add one.
    let depth = dir
        .strip_prefix(root)
        .map(|p| p.components().count())
        .unwrap_or(0);
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        // Honour .gitignore even when the folder is not a git repository:
        // an editor should not behave differently before `git init`.
        .require_git(false)
        .git_exclude(true)
        .parents(true)
        .max_depth(Some(depth + 1))
        .filter_entry({
            let dir = dir.to_path_buf();
            move |e| {
                e.path() == dir
                    || e.path().parent() == Some(dir.as_path())
                    || dir.starts_with(e.path())
            }
        })
        .build();

    for entry in walker {
        let entry = entry.map_err(|e| std::io::Error::other(e.to_string()))?;
        let path = entry.path();
        if path.parent() != Some(dir) {
            continue;
        }
        let meta = entry
            .metadata()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let rel = relative(root, path);
        out.push(Entry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            is_dir: meta.is_dir(),
            len: meta.len(),
            rel,
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// `/`-separated path of `path` relative to `root`; `""` for the root itself.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

/// Inverse of [`relative`].
pub fn absolute(root: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel.split('/').collect::<PathBuf>())
    }
}
