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
    Ok(list_children_bounded(root, dir, usize::MAX)?.0)
}

/// A bounded listing for neighbourhood expansion. `truncated` reports omitted entries.
pub fn list_children_bounded(
    root: &Path,
    dir: &Path,
    limit: usize,
) -> std::io::Result<(Vec<Entry>, bool)> {
    let mut out = Vec::new();
    let mut truncated = false;
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
        if out.len() == limit {
            truncated = true;
            break;
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
    Ok((out, truncated))
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

/// Whether `rel` is a plain relative path inside a folder (#5): `/`-separated
/// names only — no `..`, no root or drive prefix, no backslash (a separator
/// on Windows), no NUL. The empty path is the folder itself.
pub fn check_rel(rel: &str) -> Result<(), String> {
    if rel.contains(['\\', '\0']) {
        return Err(format!("bad path {rel:?}"));
    }
    for c in Path::new(rel).components() {
        match c {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => return Err(format!("path {rel:?} escapes the folder")),
        }
    }
    Ok(())
}

/// [`absolute`] for a path that must stay inside `root` (#5), which is
/// canonical. The path itself, or for one that does not exist yet its
/// nearest existing parent, must resolve inside `root` — a symlink to
/// `/etc/passwd` or a linked directory outside is refused, a link within the
/// folder is fine. A dangling link is refused too: writing through it would
/// create its target.
pub async fn jailed(root: &Path, rel: &str) -> std::io::Result<PathBuf> {
    let denied =
        |what: &str| std::io::Error::new(std::io::ErrorKind::PermissionDenied, what.to_string());
    check_rel(rel).map_err(|e| denied(&e))?;
    let path = absolute(root, rel);
    let mut probe = path.as_path();
    loop {
        match tokio::fs::canonicalize(probe).await {
            Ok(real) => {
                if !real.starts_with(root) {
                    return Err(denied(&format!("{rel:?} leads outside the folder")));
                }
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if tokio::fs::symlink_metadata(probe).await.is_ok() {
                    return Err(denied(&format!("{rel:?} is a dangling link")));
                }
                match probe.parent() {
                    Some(parent) if parent.starts_with(root) => probe = parent,
                    _ => return Err(denied(&format!("{rel:?} is outside the folder"))),
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(path)
}

/// [`jailed`] for operations on the entry itself, not what it points to
/// (rename, delete move a link, not its target): only the parent must
/// resolve inside `root`, so a link leading outside can still be removed.
pub async fn jailed_entry(root: &Path, rel: &str) -> std::io::Result<PathBuf> {
    check_rel(rel).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;
    let parent = rel.rsplit_once('/').map_or("", |(p, _)| p);
    jailed(root, parent).await?;
    Ok(absolute(root, rel))
}

/// Inverse of [`relative`]. Unchecked: user-given paths go through
/// [`jailed`].
pub fn absolute(root: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel.split('/').collect::<PathBuf>())
    }
}

/// Recovery may persist only visible, ignore-admitted regular files, with no
/// symlink component (including aliases to hidden credential files).
pub fn recovery_path(root: &Path, relative: &str) -> bool {
    if relative.len() > 4096 || check_rel(relative).is_err() {
        return false;
    }
    let parts: Vec<_> = relative.split('/').collect();
    if parts.is_empty()
        || parts.len() > 64
        || parts.iter().any(|p| p.is_empty() || p.starts_with('.'))
    {
        return false;
    }
    let mut path = root.to_path_buf();
    for part in parts {
        let Ok((entries, _)) = list_children_bounded(root, &path, 10_000) else {
            return false;
        };
        if !entries.iter().any(|entry| entry.name == part) {
            return false;
        }
        path.push(part);
        if !path
            .symlink_metadata()
            .is_ok_and(|m| !m.file_type().is_symlink())
        {
            return false;
        }
    }
    path.symlink_metadata().is_ok_and(|m| m.is_file())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn recovery_refuses_ignored_hidden_and_symlinked_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "private.txt\n").unwrap();
        for name in ["normal.txt", "private.txt", ".env"] {
            std::fs::write(dir.path().join(name), "fixture").unwrap();
        }
        assert!(recovery_path(dir.path(), "normal.txt"));
        assert!(!recovery_path(dir.path(), "private.txt"));
        assert!(!recovery_path(dir.path(), ".env"));
        assert!(!recovery_path(dir.path(), "../normal.txt"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join(".env"), dir.path().join("alias.txt"))
                .unwrap();
            assert!(!recovery_path(dir.path(), "alias.txt"));
        }
    }
}
