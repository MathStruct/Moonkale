//! Git integration types (Milestone 7): what the Changes panel asks and what
//! the platform answers. The `git` CLI runs where the folder is (desktop
//! in-process, web on the server); the workspace only sees these shapes.

use serde::{Deserialize, Serialize};

/// One changed path from `git status`. Status letters are git's: `M`
/// modified, `A` added, `D` deleted, `R` renamed, `?` untracked, `!`
/// ignored, `.` unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEntry {
    pub path: String,
    /// The old path of a rename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    /// Staged (index) status.
    pub index: char,
    /// Unstaged (worktree) status.
    pub worktree: char,
}

impl StatusEntry {
    pub fn staged(&self) -> bool {
        !matches!(self.index, '.' | '?' | '!')
    }
    pub fn unstaged(&self) -> bool {
        !matches!(self.worktree, '.' | '!') || self.index == '?'
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// ISO-8601 author date.
    pub date: String,
    pub parents: Vec<String>,
    pub subject: String,
    /// Paths touched (from `--name-only`).
    pub files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitRequest {
    Status,
    /// Unified diff of one path (staged: index vs HEAD; else worktree vs index).
    Diff {
        path: String,
        staged: bool,
    },
    Stage {
        paths: Vec<String>,
    },
    Unstage {
        paths: Vec<String>,
    },
    /// Throw away worktree changes of a path (untracked files are removed).
    Discard {
        path: String,
    },
    Commit {
        message: String,
    },
    Log {
        limit: usize,
    },
    Branches,
    Checkout {
        branch: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitResponse {
    Status {
        branch: Option<String>,
        upstream: Option<String>,
        ahead: u32,
        behind: u32,
        entries: Vec<StatusEntry>,
    },
    Diff(String),
    /// Output of a command that changes state (commit, checkout, …).
    Done(String),
    Log(Vec<Commit>),
    Branches {
        current: Option<String>,
        list: Vec<String>,
    },
    /// Not a repository / git missing: the panel shows this instead.
    Unavailable(String),
}

/// Parse `git status --porcelain=v2 --branch -z` output.
pub fn parse_status(out: &str) -> GitResponse {
    let mut branch = None;
    let mut upstream = None;
    let (mut ahead, mut behind) = (0u32, 0u32);
    let mut entries = Vec::new();
    let mut parts = out.split('\0').peekable();
    while let Some(line) = parts.next() {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some(b) = rest.strip_prefix("branch.head ") {
                branch = (b != "(detached)").then(|| b.to_string());
            } else if let Some(u) = rest.strip_prefix("branch.upstream ") {
                upstream = Some(u.to_string());
            } else if let Some(ab) = rest.strip_prefix("branch.ab ") {
                for tok in ab.split_whitespace() {
                    if let Some(n) = tok.strip_prefix('+') {
                        ahead = n.parse().unwrap_or(0);
                    } else if let Some(n) = tok.strip_prefix('-') {
                        behind = n.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        let mut fields = line.splitn(2, ' ');
        let kind = fields.next().unwrap_or("");
        let rest = fields.next().unwrap_or("");
        match kind {
            // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            "1" => {
                let cols: Vec<&str> = rest.splitn(8, ' ').collect();
                if cols.len() == 8 {
                    let xy: Vec<char> = cols[0].chars().collect();
                    entries.push(StatusEntry {
                        path: cols[7].to_string(),
                        orig_path: None,
                        index: xy.first().copied().unwrap_or('.'),
                        worktree: xy.get(1).copied().unwrap_or('.'),
                    });
                }
            }
            // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>  NUL <origPath>
            "2" => {
                let cols: Vec<&str> = rest.splitn(9, ' ').collect();
                if cols.len() == 9 {
                    let xy: Vec<char> = cols[0].chars().collect();
                    let orig = parts.next().map(str::to_string);
                    entries.push(StatusEntry {
                        path: cols[8].to_string(),
                        orig_path: orig,
                        index: xy.first().copied().unwrap_or('.'),
                        worktree: xy.get(1).copied().unwrap_or('.'),
                    });
                }
            }
            "u" => {
                let cols: Vec<&str> = rest.splitn(10, ' ').collect();
                if let Some(path) = cols.get(9) {
                    entries.push(StatusEntry {
                        path: path.to_string(),
                        orig_path: None,
                        index: 'U',
                        worktree: 'U',
                    });
                }
            }
            "?" => entries.push(StatusEntry {
                path: rest.to_string(),
                orig_path: None,
                index: '?',
                worktree: '?',
            }),
            "!" => entries.push(StatusEntry {
                path: rest.to_string(),
                orig_path: None,
                index: '!',
                worktree: '!',
            }),
            _ => {}
        }
    }
    GitResponse::Status {
        branch,
        upstream,
        ahead,
        behind,
        entries,
    }
}

/// Record and unit separators used by the log format the runner asks for.
pub const LOG_FORMAT: &str = "%x1e%H%x1f%h%x1f%an%x1f%aI%x1f%P%x1f%s";

/// Parse `git log --format=LOG_FORMAT --name-only` output.
pub fn parse_log(out: &str) -> Vec<Commit> {
    out.split('\u{1e}')
        .filter(|c| !c.trim().is_empty())
        .filter_map(|chunk| {
            let mut lines = chunk.lines();
            let head = lines.next()?;
            let f: Vec<&str> = head.split('\u{1f}').collect();
            if f.len() < 6 {
                return None;
            }
            let files = lines
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            Some(Commit {
                hash: f[0].to_string(),
                short: f[1].to_string(),
                author: f[2].to_string(),
                date: f[3].to_string(),
                parents: f[4].split_whitespace().map(str::to_string).collect(),
                subject: f[5].to_string(),
                files,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_v2_status() {
        let out = "# branch.oid abc\0# branch.head master\0# branch.upstream origin/master\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaaa bbbb README.md\0\
1 A. N... 000000 100644 100644 0000 cccc new.rs\0\
2 R. N... 100644 100644 100644 dddd dddd R100 renamed.md\0old.md\0\
? junk.txt\0";
        match parse_status(out) {
            GitResponse::Status {
                branch,
                upstream,
                ahead,
                behind,
                entries,
            } => {
                assert_eq!(branch.as_deref(), Some("master"));
                assert_eq!(upstream.as_deref(), Some("origin/master"));
                assert_eq!((ahead, behind), (2, 1));
                assert_eq!(entries.len(), 4);
                assert_eq!(
                    (
                        entries[0].index,
                        entries[0].worktree,
                        entries[0].path.as_str()
                    ),
                    ('.', 'M', "README.md")
                );
                assert!(entries[0].unstaged() && !entries[0].staged());
                assert!(entries[1].staged() && !entries[1].unstaged());
                assert_eq!(entries[2].orig_path.as_deref(), Some("old.md"));
                assert_eq!(entries[3].index, '?');
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn log_records() {
        let out = "\u{1e}abc123\u{1f}abc\u{1f}Ann\u{1f}2026-09-19T10:00:00+02:00\u{1f}def456\u{1f}Add thing\n\nsrc/main.rs\nREADME.md\n\u{1e}def456\u{1f}def\u{1f}Bob\u{1f}2026-09-18T10:00:00+02:00\u{1f}\u{1f}Initial\n\nREADME.md\n";
        let log = parse_log(out);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].files, vec!["src/main.rs", "README.md"]);
        assert_eq!(log[0].parents, vec!["def456"]);
        assert!(log[1].parents.is_empty());
        assert_eq!(log[1].subject, "Initial");
    }
}
