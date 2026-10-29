//! Everyday Git operations under cli::run's repository mutex.
use crate::cli::git;
use crate::everyday_model::{parse_blame, recovery, selection};
use crate::everyday_types::*;
use crate::types::GitResponse;
use std::path::Path;

const LIMIT: usize = 2 * 1024 * 1024;
fn bounded(s: String) -> Result<String, String> {
    if s.len() > LIMIT || s.contains('\0') {
        Err("Git text exceeds the 2 MiB limit or is binary".into())
    } else {
        Ok(s)
    }
}
async fn branch_name(root: &Path, name: &str) -> Result<(), String> {
    if name.starts_with('-') || name.is_empty() {
        return Err("Invalid branch name".into());
    }
    git(root, &["check-ref-format", &format!("refs/heads/{name}")])
        .await
        .map(|_| ())
}
pub(super) async fn local_branch(root: &Path, name: &str) -> Result<String, String> {
    branch_name(root, name).await?;
    git(
        root,
        &[
            "show-ref",
            "--verify",
            "--hash",
            &format!("refs/heads/{name}"),
        ],
    )
    .await
    .map(|s| s.trim().to_owned())
}
async fn commit(root: &Path, oid: &str) -> Result<(), String> {
    if ![40, 64].contains(&oid.len()) || !oid.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Choose a full commit identity from this repository".into());
    }
    let reachable = git(root, &["rev-list", "--all", "--reflog"]).await?;
    if !reachable.lines().any(|line| line == oid) {
        return Err("Commit does not belong to this repository's refs or reflog".into());
    }
    Ok(())
}
pub async fn state(root: &Path) -> Result<EverydayState, String> {
    let refs = git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)\t%(objectname)",
            "refs/heads/",
            "refs/remotes/",
        ],
    )
    .await?;
    let mut out = EverydayState::default();
    for line in refs.lines().take(1000) {
        if let Some((name, oid)) = line.split_once('\t') {
            out.upstreams.push(name.into());
            if let Some(name) = name.strip_prefix("refs/heads/") {
                out.branches.push(RefEntry {
                    name: name.into(),
                    oid: oid.into(),
                });
            }
        }
    }
    out.stashes = recovery(&git(root, &["stash", "list", "--format=%H%x09%gd%x09%gs"]).await?);
    out.reflog = recovery(
        &git(
            root,
            &[
                "reflog",
                "show",
                "--all",
                "-n",
                "200",
                "--format=%H%x09%gd%x09%gs",
            ],
        )
        .await?,
    );
    Ok(out)
}
async fn stash_selector(root: &Path, oid: &str) -> Result<String, String> {
    state(root)
        .await?
        .stashes
        .into_iter()
        .find(|s| s.oid == oid)
        .map(|s| s.selector)
        .ok_or("Stash changed; refresh".into())
}
fn display_status(status: &str) -> String {
    let GitResponse::Status { entries, .. } = crate::types::parse_status(status) else {
        return String::new();
    };
    entries
        .iter()
        .map(|e| format!("{}{} {}", e.index, e.worktree, e.path))
        .collect::<Vec<_>>()
        .join("\n")
}
async fn review(root: &Path, action: &EverydayAction) -> Result<EverydayPlan, String> {
    use EverydayAction::*;
    let head = git(root, &["rev-parse", "HEAD"]).await?.trim().to_owned();
    let status = git(
        root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    )
    .await?;
    let summary = match action {
        CreateBranch { name, commit: c } | Restore { name, commit: c } => {
            branch_name(root, name).await?;
            commit(root, c).await?;
            format!("Create branch {name} at {c}")
        }
        SwitchBranch { name, stash } => {
            local_branch(root, name).await?;
            format!(
                "Switch from {head} to {name}. {} Changed files: {}",
                if *stash {
                    "Save tracked and untracked changes in a stash first."
                } else {
                    "Refuse if the working tree is dirty."
                },
                display_status(&status)
            )
        }
        RenameBranch { name, to } => {
            local_branch(root, name).await?;
            branch_name(root, to).await?;
            format!("Rename branch {name} to {to}")
        }
        DeleteBranch { name } => {
            let c = local_branch(root, name).await?;
            format!("Delete branch {name} at {c}; unmerged branches are refused")
        }
        Upstream { name, upstream } => {
            local_branch(root, name).await?;
            if !state(root).await?.upstreams.contains(upstream) {
                return Err("Choose an existing repository ref".into());
            }
            format!("Set {name} upstream to {upstream}")
        }
        Revert { commit: c } | CherryPick { commit: c } => {
            commit(root, c).await?;
            format!(
                "{} {c} on {head}; conflicts use the resolution view",
                if matches!(action, Revert { .. }) {
                    "Revert"
                } else {
                    "Cherry-pick"
                }
            )
        }
        Reset { commit: c, mode } => {
            commit(root, c).await?;
            let lost = git(root, &["log", "--format=%H %s", &format!("{c}..{head}")]).await?;
            let target_paths = if *mode == ResetMode::Hard {
                bounded(
                    git(
                        root,
                        &[
                            "diff",
                            "--name-status",
                            "--no-renames",
                            "--no-ext-diff",
                            "--no-textconv",
                            &head,
                            c,
                            "--",
                        ],
                    )
                    .await?,
                )?
            } else {
                String::new()
            };
            format!("Reset {head} to {c} ({mode:?}). Commits leaving this branch:\n{lost}\n{}\n{}\nTarget paths replaced (including possible ignored obstructions):\n{target_paths}",match mode {ResetMode::Soft=>"Keep index and working files.",ResetMode::Mixed=>"Replace index; keep working files.",ResetMode::Hard=>"LOSE staged and unstaged tracked edits; Git may overwrite obstructing untracked files."},display_status(&status))
        }
        StashPush { message, untracked } => {
            if message.len() > 4096 || message.contains('\0') {
                return Err("Invalid stash message".into());
            }
            format!(
                "Stash changes (include untracked: {untracked}): {}",
                display_status(&status)
            )
        }
        StashApply { oid } | StashPop { oid } | StashDrop { oid } => {
            let selector = stash_selector(root, oid).await?;
            format!(
                "{} {selector} ({oid}). {}",
                match action {
                    StashApply { .. } => "Apply",
                    StashPop { .. } => "Pop",
                    _ => "Drop",
                },
                if matches!(action, StashDrop { .. }) {
                    "Remove this stash reference; its saved files are no longer in the stash list."
                } else {
                    "Conflicting changes remain for review; a failed pop retains the stash."
                }
            )
        }
    };
    Ok(EverydayPlan {
        repository: git(root, &["rev-parse", "--absolute-git-dir"]).await?,
        head,
        refs: git(root, &["show-ref"]).await?,
        status,
        diff: bounded(format!(
            "{}\n{}",
            git(
                root,
                &["diff", "--no-ext-diff", "--no-textconv", "--binary"]
            )
            .await?,
            git(
                root,
                &[
                    "diff",
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary"
                ]
            )
            .await?
        ))?,
        summary,
    })
}
pub async fn execute(
    root: &Path,
    action: EverydayAction,
    expected: Option<EverydayPlan>,
) -> Result<GitResponse, String> {
    let plan = review(root, &action).await?;
    let Some(expected) = expected else {
        return Ok(GitResponse::EverydayPlan(plan));
    };
    if expected != plan {
        return Err("Repository changed since review; review again".into());
    }
    use EverydayAction::*;
    let output = match action {
        CreateBranch { name, commit } | Restore { name, commit } => {
            git(root, &["branch", "--", &name, &commit]).await?
        }
        SwitchBranch { name, stash } => {
            if !plan.status.is_empty() {
                if !stash {
                    return Err("Working tree has changes; select Stash before switching".into());
                }
                git(
                    root,
                    &[
                        "stash",
                        "push",
                        "--include-untracked",
                        "-m",
                        "Moonkale before branch switch",
                    ],
                )
                .await?;
            }
            git(
                root,
                &["switch", "--no-guess", "--no-overwrite-ignore", &name],
            )
            .await?
        }
        RenameBranch { name, to } => git(root, &["branch", "-m", &name, &to]).await?,
        DeleteBranch { name } => git(root, &["branch", "-d", "--", &name]).await?,
        Upstream { name, upstream } => {
            git(
                root,
                &[
                    "branch",
                    &format!("--set-upstream-to={upstream}"),
                    "--",
                    &name,
                ],
            )
            .await?
        }
        Revert { commit } => git(root, &["revert", "--no-edit", &commit]).await?,
        CherryPick { commit } => git(root, &["cherry-pick", &commit]).await?,
        Reset { commit, mode } => {
            git(
                root,
                &[
                    "reset",
                    match mode {
                        ResetMode::Soft => "--soft",
                        ResetMode::Mixed => "--mixed",
                        ResetMode::Hard => "--hard",
                    },
                    &commit,
                    "--",
                ],
            )
            .await?
        }
        StashPush { message, untracked } => {
            let mut args = vec!["stash", "push"];
            if untracked {
                args.push("--include-untracked");
            }
            args.extend(["-m", &message]);
            git(root, &args).await?
        }
        StashApply { oid } => {
            let s = stash_selector(root, &oid).await?;
            git(root, &["stash", "apply", "--index", &s]).await?
        }
        StashPop { oid } => {
            let s = stash_selector(root, &oid).await?;
            git(root, &["stash", "pop", "--index", &s]).await?
        }
        StashDrop { oid } => {
            let s = stash_selector(root, &oid).await?;
            git(root, &["stash", "drop", &s]).await?
        }
    };
    Ok(GitResponse::Done(if output.trim().is_empty() {
        plan.summary
    } else {
        output
    }))
}
pub async fn compare(root: &Path, left: &str, right: &str) -> Result<GitResponse, String> {
    local_branch(root, left).await?;
    local_branch(root, right).await?;
    Ok(GitResponse::Diff(bounded(
        git(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                &format!("refs/heads/{left}"),
                &format!("refs/heads/{right}"),
                "--",
            ],
        )
        .await?,
    )?))
}
pub async fn stash_show(root: &Path, oid: &str) -> Result<GitResponse, String> {
    let s = stash_selector(root, oid).await?;
    Ok(GitResponse::Diff(bounded(
        git(
            root,
            &[
                "stash",
                "show",
                "--include-untracked",
                "--patch",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                &s,
            ],
        )
        .await?,
    )?))
}
pub async fn blame(root: &Path, path: &str, at: Option<&str>) -> Result<GitResponse, String> {
    crate::cli::check_path(path)?;
    let mut args = vec!["blame", "--line-porcelain"];
    if let Some(c) = at {
        commit(root, c).await?;
        args.push(c);
    }
    args.extend(["--", path]);
    let text = bounded(git(root, &args).await?)?;
    let result = parse_blame(&text)?;
    Ok(GitResponse::Blame(result))
}

pub async fn stage(
    root: &Path,
    path: &str,
    staged: bool,
    expected: &str,
    lines: &[usize],
) -> Result<GitResponse, String> {
    crate::cli::check_path(path)?;
    if expected.len() > LIMIT || lines.len() > 100_000 {
        return Err("Selection too large".into());
    }
    let current = bounded(crate::cli::diff_text(root, path, staged).await?)?;
    if current != expected {
        return Err("Diff changed; refresh before staging".into());
    }
    let patch = selection(&current, lines, staged)?;
    let mut file = tempfile::NamedTempFile::new_in(root.join(".git"))
        .or_else(|_| tempfile::NamedTempFile::new())
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(patch.as_bytes())
        .map_err(|e| e.to_string())?;
    let patch_path = file.path().to_str().ok_or("Invalid patch path")?;
    let mut args = vec!["apply", "--cached", "--recount", "--whitespace=nowarn"];
    args.extend(["--", patch_path]);
    git(root, &args).await?;
    Ok(GitResponse::Done(format!(
        "{} {path}",
        if staged {
            "Unstaged selection in"
        } else {
            "Staged selection in"
        }
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::run;
    use crate::types::GitRequest;
    use std::process::Command;
    fn sh(root: &Path, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8(o.stdout).unwrap()
    }
    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        sh(p, &["init", "-q", "-b", "main"]);
        sh(p, &["config", "user.name", "T"]);
        sh(p, &["config", "user.email", "t@x"]);
        std::fs::write(p.join("a"), "one\ntwo\nthree\n").unwrap();
        sh(p, &["add", "a"]);
        sh(p, &["commit", "-qm", "initial"]);
        d
    }
    async fn reviewed(root: &Path, action: EverydayAction) -> Result<GitResponse, String> {
        let GitResponse::EverydayPlan(plan) = run(
            root,
            GitRequest::Everyday {
                action: action.clone(),
                expected: None,
            },
        )
        .await?
        else {
            panic!()
        };
        run(
            root,
            GitRequest::Everyday {
                action,
                expected: Some(plan),
            },
        )
        .await
    }
    async fn diff(root: &Path, staged: bool) -> String {
        let GitResponse::Diff(s) = run(
            root,
            GitRequest::Diff {
                path: "a".into(),
                staged,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        s
    }
    async fn select(
        root: &Path,
        staged: bool,
        text: &str,
        lines: Vec<usize>,
    ) -> Result<GitResponse, String> {
        run(
            root,
            GitRequest::StageSelection {
                path: "a".into(),
                staged,
                expected: text.into(),
                lines,
            },
        )
        .await
    }
    #[tokio::test]
    async fn stages_and_unstages_hunks_and_lines_preserving_crlf_and_eof() {
        for (before, after) in [
            ("one\ntwo\nthree\n", "ONE\ntwo\nTHREE\n"),
            ("one\r\ntwo\r\nthree\r\n", "ONE\r\ntwo\r\nTHREE\r\n"),
            ("one\ntwo\nthree", "ONE\ntwo\nTHREE"),
        ] {
            let d = repo();
            let p = d.path();
            std::fs::write(p.join("a"), before).unwrap();
            sh(p, &["add", "a"]);
            sh(p, &["commit", "--allow-empty", "-qm", "base"]);
            std::fs::write(p.join("a"), after).unwrap();
            let text = diff(p, false).await;
            let one = text.lines().position(|l| l.starts_with("+ONE")).unwrap();
            select(p, false, &text, vec![one]).await.unwrap();
            assert!(sh(p, &["show", ":a"]).contains("ONE"));
            assert!(sh(p, &["show", ":a"]).contains("three"));
            let staged = diff(p, true).await;
            let n = staged.lines().position(|l| l.starts_with("+ONE")).unwrap();
            select(p, true, &staged, vec![n]).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), before);
            let text = diff(p, false).await;
            let all = text
                .lines()
                .enumerate()
                .filter(|(_, l)| {
                    (l.starts_with('+') && !l.starts_with("+++"))
                        || (l.starts_with('-') && !l.starts_with("---"))
                })
                .map(|(i, _)| i)
                .collect();
            select(p, false, &text, all).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), after);
            let staged = diff(p, true).await;
            let all = staged
                .lines()
                .enumerate()
                .filter(|(_, l)| {
                    (l.starts_with('+') && !l.starts_with("+++"))
                        || (l.starts_with('-') && !l.starts_with("---"))
                })
                .map(|(i, _)| i)
                .collect();
            select(p, true, &staged, all).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), before);
            assert_eq!(std::fs::read_to_string(p.join("a")).unwrap(), after);
            assert!(select(p, false, "stale", vec![one]).await.is_err());
        }
    }
    #[tokio::test]
    async fn separate_hunks_and_unsafe_selection() {
        let d = repo();
        let p = d.path();
        let before = (0..30).map(|i| format!("line {i}\n")).collect::<String>();
        std::fs::write(p.join("a"), &before).unwrap();
        sh(p, &["add", "a"]);
        sh(p, &["commit", "-qm", "many"]);
        let after = before
            .replace("line 1\n", "changed 1\n")
            .replace("line 27\n", "changed 27\n");
        std::fs::write(p.join("a"), &after).unwrap();
        let text = diff(p, false).await;
        assert_eq!(text.lines().filter(|l| l.starts_with("@@ ")).count(), 2);
        let lines = text
            .lines()
            .enumerate()
            .skip_while(|(_, l)| !l.starts_with("@@ "))
            .skip(1)
            .take_while(|(_, l)| !l.starts_with("@@ "))
            .filter(|(_, l)| l.starts_with('+') || l.starts_with('-'))
            .map(|(i, _)| i)
            .collect();
        select(p, false, &text, lines).await.unwrap();
        let index = sh(p, &["show", ":a"]);
        assert!(index.contains("changed 1"));
        assert!(index.contains("line 27"));
        assert!(selection(&text, &[], false).is_err());
        assert!(run(
            p,
            GitRequest::StageSelection {
                path: "../outside".into(),
                staged: false,
                expected: text,
                lines: vec![1]
            }
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn stash_untracked_apply_pop_drop_and_dirty_switch() {
        let d = repo();
        let p = d.path();
        let head = sh(p, &["rev-parse", "HEAD"]).trim().to_owned();
        reviewed(
            p,
            EverydayAction::CreateBranch {
                name: "other".into(),
                commit: head,
            },
        )
        .await
        .unwrap();
        std::fs::write(p.join("a"), "dirty\n").unwrap();
        std::fs::write(p.join("untracked"), "keep me").unwrap();
        assert!(reviewed(
            p,
            EverydayAction::SwitchBranch {
                name: "other".into(),
                stash: false
            }
        )
        .await
        .is_err());
        reviewed(
            p,
            EverydayAction::SwitchBranch {
                name: "other".into(),
                stash: true,
            },
        )
        .await
        .unwrap();
        assert!(
            !p.join("untracked").exists(),
            "status={} stash={}",
            sh(p, &["status", "--porcelain=v2"]),
            sh(p, &["stash", "list"])
        );
        let stash = state(p).await.unwrap().stashes[0].oid.clone();
        assert!(
            matches!(stash_show(p,&stash).await.unwrap(),GitResponse::Diff(s) if s.contains("keep me"))
        );
        reviewed(p, EverydayAction::StashApply { oid: stash.clone() })
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(p.join("untracked")).unwrap(),
            "keep me"
        );
        sh(p, &["restore", "a"]);
        std::fs::remove_file(p.join("untracked")).unwrap();
        reviewed(p, EverydayAction::StashPop { oid: stash })
            .await
            .unwrap();
        assert!(state(p).await.unwrap().stashes.is_empty());
        reviewed(
            p,
            EverydayAction::StashPush {
                message: "test untracked".into(),
                untracked: true,
            },
        )
        .await
        .unwrap();
        let stash = state(p).await.unwrap().stashes[0].oid.clone();
        reviewed(p, EverydayAction::StashDrop { oid: stash })
            .await
            .unwrap();
        assert!(state(p).await.unwrap().stashes.is_empty());
    }
    #[tokio::test]
    async fn reset_requires_fresh_review_and_reflog_restores_lost_commit() {
        let d = repo();
        let p = d.path();
        let base = sh(p, &["rev-parse", "HEAD"]).trim().to_owned();
        std::fs::write(p.join("a"), "second\n").unwrap();
        sh(p, &["add", "a"]);
        sh(p, &["commit", "-qm", "second"]);
        let tip = sh(p, &["rev-parse", "HEAD"]).trim().to_owned();
        std::fs::write(p.join("a"), "unsaved tracked text\n").unwrap();
        let action = EverydayAction::Reset {
            commit: base.clone(),
            mode: ResetMode::Hard,
        };
        let GitResponse::EverydayPlan(plan) = run(
            p,
            GitRequest::Everyday {
                action: action.clone(),
                expected: None,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        assert!(plan.summary.contains(&tip));
        assert!(plan.summary.contains("LOSE"));
        assert!(plan.summary.contains("a"));
        assert_eq!(sh(p, &["rev-parse", "HEAD"]).trim(), tip);
        std::fs::write(p.join("a"), "changed since review\n").unwrap();
        assert!(run(
            p,
            GitRequest::Everyday {
                action: action.clone(),
                expected: Some(plan)
            }
        )
        .await
        .is_err());
        reviewed(p, action).await.unwrap();
        assert_eq!(sh(p, &["rev-parse", "HEAD"]).trim(), base);
        assert!(state(p).await.unwrap().reflog.iter().any(|r| r.oid == tip));
        reviewed(
            p,
            EverydayAction::Restore {
                commit: tip.clone(),
                name: "recovered".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(local_branch(p, "recovered").await.unwrap(), tip);
        for mode in [ResetMode::Soft, ResetMode::Mixed] {
            reviewed(
                p,
                EverydayAction::Reset {
                    commit: tip.clone(),
                    mode,
                },
            )
            .await
            .unwrap();
            assert_eq!(
                std::fs::read_to_string(p.join("a")).unwrap(),
                "one\ntwo\nthree\n"
            );
        }
        assert!(reviewed(
            p,
            EverydayAction::Reset {
                commit: "HEAD~1".into(),
                mode: ResetMode::Hard
            }
        )
        .await
        .is_err());
        assert!(reviewed(
            p,
            EverydayAction::Restore {
                commit: "f".repeat(40),
                name: "foreign".into()
            }
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn branches_upstream_compare_blame_revert_cherry_pick() {
        let d = repo();
        let p = d.path();
        let base = sh(p, &["rev-parse", "HEAD"]).trim().to_owned();
        reviewed(
            p,
            EverydayAction::CreateBranch {
                name: "topic".into(),
                commit: base.clone(),
            },
        )
        .await
        .unwrap();
        reviewed(
            p,
            EverydayAction::SwitchBranch {
                name: "topic".into(),
                stash: false,
            },
        )
        .await
        .unwrap();
        std::fs::write(p.join("a"), "ONE\ntwo\nthree\n").unwrap();
        sh(p, &["add", "a"]);
        sh(p, &["commit", "-qm", "change first line"]);
        let tip = sh(p, &["rev-parse", "HEAD"]).trim().to_owned();
        let GitResponse::Blame(rows) = blame(p, "a", None).await.unwrap() else {
            panic!()
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].commit, tip);
        assert_eq!(rows[0].previous, Some(base.clone()));
        let GitResponse::Blame(old) = blame(p, "a", Some(&base)).await.unwrap() else {
            panic!()
        };
        assert_eq!(old[0].commit, base);
        assert!(
            matches!(compare(p,"main","topic").await.unwrap(),GitResponse::Diff(d) if d.contains("+ONE"))
        );
        reviewed(
            p,
            EverydayAction::Upstream {
                name: "topic".into(),
                upstream: "refs/heads/main".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            sh(p, &["rev-parse", "--abbrev-ref", "topic@{upstream}"]).trim(),
            "main"
        );
        reviewed(
            p,
            EverydayAction::RenameBranch {
                name: "topic".into(),
                to: "renamed".into(),
            },
        )
        .await
        .unwrap();
        reviewed(
            p,
            EverydayAction::SwitchBranch {
                name: "main".into(),
                stash: false,
            },
        )
        .await
        .unwrap();
        reviewed(
            p,
            EverydayAction::CherryPick {
                commit: tip.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(p.join("a")).unwrap(),
            "ONE\ntwo\nthree\n"
        );
        reviewed(
            p,
            EverydayAction::Revert {
                commit: sh(p, &["rev-parse", "HEAD"]).trim().into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(p.join("a")).unwrap(),
            "one\ntwo\nthree\n"
        );
        // A cherry-pick need not retain the original commit identity (timestamps
        // differ). Refuse deleting that unmerged branch, then delete a merged one.
        let current = sh(p, &["rev-parse", "HEAD"]).trim().to_owned();
        reviewed(
            p,
            EverydayAction::CreateBranch {
                name: "deletable".into(),
                commit: current,
            },
        )
        .await
        .unwrap();
        reviewed(
            p,
            EverydayAction::DeleteBranch {
                name: "deletable".into(),
            },
        )
        .await
        .unwrap();
        assert!(local_branch(p, "deletable").await.is_err());
        for name in ["-bad", "../escape", "main^{tree}"] {
            assert!(local_branch(p, name).await.is_err());
        }
    }
    #[tokio::test]
    async fn new_and_deleted_files_support_partial_lines_and_hunks_both_ways() {
        for text in [
            "one\ntwo\nthree\n",
            "one\r\ntwo\r\nthree\r\n",
            "one\ntwo\nthree",
        ] {
            let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
            let two = format!("two{ending}");
            let remaining = text.replace(&two, "");
            let d = repo();
            let p = d.path();
            sh(p, &["rm", "a"]);
            sh(p, &["commit", "-qm", "remove base file"]);
            std::fs::write(p.join("a"), text).unwrap();
            let current = diff(p, false).await;
            let line = current.lines().position(|l| l.starts_with("+two")).unwrap();
            select(p, false, &current, vec![line]).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), two);
            let current = diff(p, true).await;
            let line = current.lines().position(|l| l.starts_with("+two")).unwrap();
            select(p, true, &current, vec![line]).await.unwrap();
            assert!(sh(p, &["ls-files", "--", "a"]).is_empty());
            let current = diff(p, false).await;
            let lines = current
                .lines()
                .enumerate()
                .filter(|(_, l)| l.starts_with('+') && !l.starts_with("+++"))
                .map(|(i, _)| i)
                .collect();
            select(p, false, &current, lines).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), text);
            let current = diff(p, true).await;
            let line = current.lines().position(|l| l.starts_with("+two")).unwrap();
            select(p, true, &current, vec![line]).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), remaining);
            let current = diff(p, true).await;
            let lines = current
                .lines()
                .enumerate()
                .filter(|(_, l)| l.starts_with('+') && !l.starts_with("+++"))
                .map(|(i, _)| i)
                .collect();
            select(p, true, &current, lines).await.unwrap();
            assert!(sh(p, &["ls-files", "--", "a"]).is_empty());
            assert_eq!(std::fs::read_to_string(p.join("a")).unwrap(), text);

            let d = repo();
            let p = d.path();
            std::fs::write(p.join("a"), text).unwrap();
            sh(p, &["add", "a"]);
            sh(p, &["commit", "--allow-empty", "-qm", "line endings"]);
            std::fs::remove_file(p.join("a")).unwrap();
            let current = diff(p, false).await;
            let line = current.lines().position(|l| l.starts_with("-two")).unwrap();
            select(p, false, &current, vec![line]).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), remaining);
            let current = diff(p, true).await;
            let line = current.lines().position(|l| l.starts_with("-two")).unwrap();
            select(p, true, &current, vec![line]).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), text);
            let current = diff(p, false).await;
            let lines = current
                .lines()
                .enumerate()
                .filter(|(_, l)| l.starts_with('-') && !l.starts_with("---"))
                .map(|(i, _)| i)
                .collect();
            select(p, false, &current, lines).await.unwrap();
            assert!(sh(p, &["ls-files", "--", "a"]).is_empty());
            let current = diff(p, true).await;
            let line = current.lines().position(|l| l.starts_with("-two")).unwrap();
            select(p, true, &current, vec![line]).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), two);
            let current = diff(p, true).await;
            let lines = current
                .lines()
                .enumerate()
                .filter(|(_, l)| l.starts_with('-') && !l.starts_with("---"))
                .map(|(i, _)| i)
                .collect();
            select(p, true, &current, lines).await.unwrap();
            assert_eq!(sh(p, &["show", ":a"]), text);
            assert!(!p.join("a").exists());
        }
    }

    #[tokio::test]
    async fn quoted_new_file_paths_keep_literal_prefixes() {
        let d = repo();
        let p = d.path();
        std::fs::create_dir(p.join("b")).unwrap();
        let name = "b/a quoted\tfile";
        std::fs::write(p.join(name), "first\nsecond\n").unwrap();
        let current = crate::cli::diff_text(p, name, false).await.unwrap();
        let line = current
            .lines()
            .position(|l| l.starts_with("+second"))
            .unwrap();
        run(
            p,
            GitRequest::StageSelection {
                path: name.into(),
                staged: false,
                expected: current,
                lines: vec![line],
            },
        )
        .await
        .unwrap();
        assert_eq!(sh(p, &["show", &format!(":{name}")]), "second\n");
    }
    #[tokio::test]
    async fn switching_protects_ignored_files_and_hard_review_names_them() {
        let d = repo();
        let root = d.path();
        sh(root, &["switch", "-c", "other"]);
        std::fs::write(root.join("ignored.txt"), "incoming\n").unwrap();
        sh(root, &["add", "ignored.txt"]);
        sh(root, &["commit", "-qm", "incoming file"]);
        let incoming = sh(root, &["rev-parse", "HEAD"]).trim().to_owned();
        sh(root, &["switch", "main"]);
        std::fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        sh(root, &["add", ".gitignore"]);
        sh(root, &["commit", "-qm", "ignore local data"]);
        std::fs::write(root.join("ignored.txt"), "local data\n").unwrap();
        assert!(reviewed(
            root,
            EverydayAction::SwitchBranch {
                name: "other".into(),
                stash: false
            }
        )
        .await
        .is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("ignored.txt")).unwrap(),
            "local data\n"
        );
        let action = EverydayAction::Reset {
            commit: incoming,
            mode: ResetMode::Hard,
        };
        let GitResponse::EverydayPlan(plan) = run(
            root,
            GitRequest::Everyday {
                action: action.clone(),
                expected: None,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        assert!(plan.summary.contains("ignored.txt"));
        assert!(plan.summary.contains("LOSE"));
        run(
            root,
            GitRequest::Everyday {
                action,
                expected: Some(plan),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("ignored.txt")).unwrap(),
            "incoming\n"
        );
    }
    #[tokio::test]
    async fn linked_worktrees_wait_for_the_same_repository_lock() {
        let d = repo();
        let root = d.path();
        let worktree = d.path().join("linked");
        sh(
            root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                worktree.to_str().unwrap(),
            ],
        );
        let common = git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .await
        .unwrap();
        let lock = crate::sync::repo_lock(common.trim());
        let guard = lock.lock().await;
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(250),
            run(&worktree, GitRequest::Status)
        )
        .await
        .is_err());
        drop(guard);
        assert!(
            matches!(tokio::time::timeout(std::time::Duration::from_secs(5),run(&worktree,GitRequest::Status)).await.unwrap().unwrap(), GitResponse::Status { branch: Some(name), .. } if name == "linked")
        );
    }
}
