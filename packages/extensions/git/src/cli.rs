//! Runs the `git` binary for one repository (feature `cli`). Every
//! command is confined to `root` (`-C`); paths are checked against `..`.

use moonkale_ext_api::git::{parse_log, parse_status, GitRequest, GitResponse, LOG_FORMAT};
use std::path::Path;

async fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .output()
        .await
        .map_err(|e| format!("cannot run git: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { stdout } else { err })
    }
}

fn check_path(p: &str) -> Result<&str, String> {
    if p.is_empty() || p.starts_with('/') || p.split('/').any(|s| s == "..") {
        return Err(format!("bad path {p:?}"));
    }
    Ok(p)
}

/// Run one request in `root`. A missing repository or binary comes back as
/// [`GitResponse::Unavailable`] rather than an error.
pub async fn run(root: &Path, req: GitRequest) -> Result<GitResponse, String> {
    if !root.join(".git").exists() {
        // Sub-directories of a repository are fine too: ask git.
        match git(root, &["rev-parse", "--show-toplevel"]).await {
            Ok(_) => {}
            Err(e) => {
                return Ok(GitResponse::Unavailable(
                    if e.contains("not a git repository") {
                        "Not a git repository".into()
                    } else {
                        e
                    },
                ))
            }
        }
    }
    match req {
        GitRequest::Status => {
            let out = git(
                root,
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "-z",
                    "--untracked-files=all",
                ],
            )
            .await?;
            Ok(parse_status(&out))
        }
        GitRequest::Diff { path, staged } => {
            let path = check_path(&path)?;
            let out = if staged {
                git(root, &["diff", "--cached", "--no-color", "--", path]).await?
            } else {
                // Untracked files have no diff: show them as all-added.
                let d = git(root, &["diff", "--no-color", "--", path]).await?;
                if d.is_empty()
                    && git(root, &["ls-files", "--error-unmatch", "--", path])
                        .await
                        .is_err()
                {
                    // `--no-index` exits 1 when there are differences, so the
                    // diff arrives on the error side.
                    match git(
                        root,
                        &["diff", "--no-color", "--no-index", "--", "/dev/null", path],
                    )
                    .await
                    {
                        Ok(d) => d,
                        Err(e) if e.starts_with("diff ") => e,
                        Err(e) => return Err(e),
                    }
                } else {
                    d
                }
            };
            Ok(GitResponse::Diff(out))
        }
        GitRequest::Stage { paths } => {
            let mut args = vec!["add", "-A", "--"];
            for p in &paths {
                args.push(check_path(p)?);
            }
            git(root, &args).await?;
            Ok(GitResponse::Done(format!("staged {}", paths.len())))
        }
        GitRequest::Unstage { paths } => {
            let mut args = vec!["restore", "--staged", "--"];
            for p in &paths {
                args.push(check_path(p)?);
            }
            git(root, &args).await?;
            Ok(GitResponse::Done(format!("unstaged {}", paths.len())))
        }
        GitRequest::Discard { path } => {
            let path = check_path(&path)?;
            if git(root, &["ls-files", "--error-unmatch", "--", path])
                .await
                .is_ok()
            {
                git(root, &["restore", "--worktree", "--", path]).await?;
            } else {
                git(root, &["clean", "-f", "--", path]).await?;
            }
            Ok(GitResponse::Done(format!("discarded {path}")))
        }
        GitRequest::Commit { message } => {
            if message.trim().is_empty() {
                return Err("a commit needs a message".into());
            }
            let out = git(root, &["commit", "-m", &message]).await?;
            Ok(GitResponse::Done(
                out.lines().next().unwrap_or("committed").to_string(),
            ))
        }
        GitRequest::Log { limit } => {
            let n = limit.clamp(1, 500).to_string();
            let format = format!("--format={LOG_FORMAT}");
            let out = git(root, &["log", "-n", &n, &format, "--name-only"]).await?;
            Ok(GitResponse::Log(parse_log(&out)))
        }
        GitRequest::Branches => {
            let out = git(root, &["branch", "--format=%(HEAD)%(refname:short)"]).await?;
            let mut current = None;
            let mut list = Vec::new();
            for line in out.lines() {
                let (head, name) = line.split_at(1);
                if head == "*" {
                    current = Some(name.to_string());
                }
                list.push(name.to_string());
            }
            Ok(GitResponse::Branches { current, list })
        }
        GitRequest::Checkout { branch } => {
            if branch.starts_with('-') {
                return Err("bad branch name".into());
            }
            let out = git(root, &["checkout", &branch]).await?;
            Ok(GitResponse::Done(
                out.lines().next().unwrap_or("switched").to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let sh = |args: &[&str]| {
            let st = Command::new("git")
                .arg("-C")
                .arg(p)
                .args(args)
                .env("GIT_AUTHOR_NAME", "T")
                .env("GIT_AUTHOR_EMAIL", "t@x")
                .env("GIT_COMMITTER_NAME", "T")
                .env("GIT_COMMITTER_EMAIL", "t@x")
                .status()
                .unwrap();
            assert!(st.success(), "git {args:?}");
        };
        sh(&["init", "-q", "-b", "main"]);
        std::fs::write(p.join("README.md"), "one\n").unwrap();
        sh(&["add", "README.md"]);
        sh(&[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@x",
            "commit",
            "-q",
            "-m",
            "first",
        ]);
        dir
    }

    #[tokio::test]
    async fn status_stage_commit_log_round_trip() {
        let dir = repo();
        let p = dir.path();
        std::fs::write(p.join("README.md"), "one\ntwo\n").unwrap();
        std::fs::write(p.join("new.txt"), "n\n").unwrap();
        let GitResponse::Status {
            branch, entries, ..
        } = run(p, GitRequest::Status).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(entries.len(), 2);
        let GitResponse::Diff(d) = run(
            p,
            GitRequest::Diff {
                path: "README.md".into(),
                staged: false,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        assert!(d.contains("+two"));
        let GitResponse::Diff(u) = run(
            p,
            GitRequest::Diff {
                path: "new.txt".into(),
                staged: false,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        assert!(u.contains("+n"), "{u}");
        run(
            p,
            GitRequest::Stage {
                paths: vec!["README.md".into(), "new.txt".into()],
            },
        )
        .await
        .unwrap();
        let GitResponse::Status { entries, .. } = run(p, GitRequest::Status).await.unwrap() else {
            panic!()
        };
        assert!(entries.iter().all(|e| e.staged()));
        run(
            p,
            GitRequest::Unstage {
                paths: vec!["new.txt".into()],
            },
        )
        .await
        .unwrap();
        // Commit needs an identity: set it in the repo config.
        Command::new("git")
            .arg("-C")
            .arg(p)
            .args(["config", "user.name", "T"])
            .status()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(p)
            .args(["config", "user.email", "t@x"])
            .status()
            .unwrap();
        run(
            p,
            GitRequest::Commit {
                message: "second".into(),
            },
        )
        .await
        .unwrap();
        let GitResponse::Log(log) = run(p, GitRequest::Log { limit: 10 }).await.unwrap() else {
            panic!()
        };
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].subject, "second");
        assert_eq!(log[0].files, vec!["README.md"]);
        assert_eq!(log[0].parents, vec![log[1].hash.clone()]);
        run(
            p,
            GitRequest::Discard {
                path: "new.txt".into(),
            },
        )
        .await
        .unwrap();
        assert!(!p.join("new.txt").exists());
        assert!(run(
            p,
            GitRequest::Diff {
                path: "../x".into(),
                staged: false
            }
        )
        .await
        .is_err());
        let GitResponse::Branches { current, .. } = run(p, GitRequest::Branches).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(current.as_deref(), Some("main"));
    }

    #[tokio::test]
    async fn not_a_repository_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        // A temp dir under /tmp is not inside a repository.
        match run(dir.path(), GitRequest::Status).await.unwrap() {
            GitResponse::Unavailable(_) => {}
            other => panic!("{other:?}"),
        }
    }
}
