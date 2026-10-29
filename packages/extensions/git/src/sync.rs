//! Server-owned sync policy and conflict I/O. No client-supplied remote or argv.
use crate::cli::git;
use crate::types::*;
use std::path::{Path, PathBuf};

pub(super) fn repo_lock(key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::sync::{Arc, Mutex, OnceLock, Weak};
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<String, Weak<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, v| v.strong_count() > 0);
    if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(key.into(), Arc::downgrade(&lock));
    lock
}

const MAX_TEXT: u64 = 2 * 1024 * 1024;

async fn config(root: &Path, key: &str) -> String {
    git(root, &["config", "--get", key])
        .await
        .unwrap_or_default()
        .trim()
        .into()
}

/// Reject embedded HTTP credentials before Git can echo an endpoint in diagnostics.
fn credential_free(url: &str) -> bool {
    if let Some((scheme, rest)) = url.split_once("://") {
        let authority = rest.split('/').next().unwrap_or(rest);
        if scheme != "ssh" && authority.contains('@') {
            return false;
        }
        if scheme == "ssh" && authority.split('@').next().is_some_and(|s| s.contains(':')) {
            return false;
        }
    }
    !url.contains('?') && !url.contains('#') && !url.contains('\n')
}

fn secure_transport(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        // A local path or SSH scp syntax; never a `transport::address` remote
        // helper (`ext::` can run a command), whatever protocol.allow says.
        return !url.contains("::");
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    match scheme {
        "https" | "ssh" => true,
        "file" => authority.is_empty() || authority == "localhost",
        "http" => {
            let host = authority.split(':').next().unwrap_or_default();
            matches!(host, "localhost" | "127.0.0.1")
                || authority == "[::1]"
                || authority.starts_with("[::1]:")
        }
        _ => false,
    }
}

async fn plan(root: &Path) -> Result<SyncPlan, String> {
    let head = git(root, &["rev-parse", "HEAD"]).await?.trim().into();
    let branch = git(root, &["symbolic-ref", "--short", "HEAD"])
        .await?
        .trim()
        .to_string();
    let status = git(root, &["status", "--porcelain=v2", "--branch", "-z"]).await?;
    let GitResponse::Status {
        ahead,
        behind,
        entries,
        ..
    } = parse_status(&status)
    else {
        unreachable!()
    };
    let mut remote = config(root, &format!("branch.{branch}.remote")).await;
    if remote.is_empty() {
        let remotes = git(root, &["remote"]).await?;
        let names: Vec<_> = remotes.lines().collect();
        remote = if names.contains(&"origin") {
            "origin".into()
        } else if names.len() == 1 {
            names[0].into()
        } else {
            return Err(
                "Choose a repository remote in Git configuration before publishing.".into(),
            );
        };
    }
    if remote.starts_with('-') || remote == "." || remote.contains(['/', ':', '\n']) {
        return Err("A configured named remote is required.".into());
    }
    // Includes pushurl and URL rewrite expansion. Neither the endpoint nor credentials
    // are returned to the client. Helpers and ssh-agent are inherited from the system.
    use std::hash::{Hash, Hasher};
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    for args in [
        vec!["remote", "get-url", "--all", &remote],
        vec!["remote", "get-url", "--push", "--all", &remote],
    ] {
        let urls = git(root, &args).await?;
        urls.hash(&mut identity);
        if urls.lines().any(|u| !credential_free(u)) {
            return Err("Remove embedded credentials from Git remote URLs; use the system credential helper or ssh-agent.".into());
        }
        if urls.lines().any(|u| !secure_transport(u)) {
            return Err("Git remotes require HTTPS or SSH off loopback.".into());
        }
    }
    let mut target = config(root, &format!("branch.{branch}.merge")).await;
    if target.is_empty() {
        target = format!("refs/heads/{branch}");
    }
    git(root, &["check-ref-format", &target]).await?;
    let remote_oid = git(root, &["rev-parse", "--verify", "@{upstream}"])
        .await
        .unwrap_or_default()
        .trim()
        .into();
    Ok(SyncPlan {
        repository: root
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
        remote_identity: format!("{:016x}", identity.finish()),
        head,
        status,
        remote,
        branch,
        target,
        remote_oid,
        ahead,
        behind,
        dirty: !entries.is_empty(),
    })
}

fn push_args(p: &SyncPlan, action: SyncAction) -> Vec<String> {
    let mut args = vec!["push".into(), "--progress".into()];
    if action == SyncAction::PushLease {
        args.push(format!("--force-with-lease={}:{}", p.target, p.remote_oid));
    }
    if action == SyncAction::Publish {
        args.push("--set-upstream".into());
    }
    args.extend([p.remote.clone(), format!("HEAD:{}", p.target)]);
    args
}

type Output = std::sync::Arc<std::sync::Mutex<Vec<u8>>>;
type OutputMap = std::sync::Mutex<
    std::collections::HashMap<PathBuf, std::sync::Weak<std::sync::Mutex<Vec<u8>>>>,
>;
fn outputs() -> &'static OutputMap {
    static OUTPUTS: std::sync::OnceLock<OutputMap> = std::sync::OnceLock::new();
    OUTPUTS.get_or_init(Default::default)
}
pub(super) fn progress(root: &Path) -> String {
    let output = outputs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root)
        .and_then(std::sync::Weak::upgrade);
    output
        .map(|o| String::from_utf8_lossy(&o.lock().unwrap_or_else(|e| e.into_inner())).into_owned())
        .unwrap_or_default()
}
async fn drain(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    output: Output,
) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let mut buf = [0; 4096];
    loop {
        let n = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        let mut out = output.lock().unwrap_or_else(|e| e.into_inner());
        if out.len() + n > 1024 * 1024 {
            return Err("Git output exceeds 1 MiB".into());
        }
        out.extend_from_slice(&buf[..n]);
    }
}
async fn network(root: &Path, args: &[String]) -> Result<String, String> {
    let output: Output = Default::default();
    {
        let mut outputs = outputs().lock().unwrap_or_else(|e| e.into_inner());
        outputs.retain(|_, v| v.strong_count() > 0);
        outputs.insert(root.into(), std::sync::Arc::downgrade(&output));
    }
    let mut child = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("LC_ALL", "C")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Missing stdout")?;
    let stderr = child.stderr.take().ok_or("Missing stderr")?;
    let (out, err, status) = tokio::time::timeout(std::time::Duration::from_secs(300), async {
        tokio::join!(
            drain(stdout, output.clone()),
            drain(stderr, output.clone()),
            child.wait()
        )
    })
    .await
    .map_err(|_| "Git operation timed out after five minutes".to_string())?;
    out?;
    err?;
    let text =
        String::from_utf8_lossy(&output.lock().unwrap_or_else(|e| e.into_inner())).into_owned();
    if status.map_err(|e| e.to_string())?.success() {
        Ok(text)
    } else {
        Err(text)
    }
}

pub async fn sync(
    root: &Path,
    action: SyncAction,
    expected: Option<SyncPlan>,
) -> Result<GitResponse, String> {
    let mut p = plan(root).await?;
    // Pull's review must describe the remote as it is now, not a stale
    // tracking ref. Lease review deliberately keeps its previously fetched OID.
    if expected.is_none() && matches!(action, SyncAction::Pull | SyncAction::Sync) {
        network(
            root,
            &["fetch".into(), "--progress".into(), p.remote.clone()],
        )
        .await?;
        p = plan(root).await?;
    }
    if action == SyncAction::Fetch {
        return network(root, &["fetch".into(), "--progress".into(), p.remote])
            .await
            .map(GitResponse::Done);
    }
    let Some(expected) = expected else {
        return Ok(GitResponse::SyncPlan(p));
    };
    if expected != p {
        return Err("Repository changed. Review sync again before continuing.".into());
    }
    if operation(root).await.is_some() {
        return Err("Resolve or abort the current Git operation first.".into());
    }
    if matches!(action, SyncAction::Pull | SyncAction::Sync) && p.dirty {
        return Err("Working tree is dirty. Commit or stash changes before pulling.".into());
    }
    let mut output = String::new();
    if matches!(action, SyncAction::Pull | SyncAction::Sync) {
        // Deliberately no --rebase/--no-rebase: the repository's own pull policy wins.
        output.push_str(
            &network(
                root,
                &["pull".into(), "--no-edit".into(), "--progress".into()],
            )
            .await?,
        );
    }
    if matches!(
        action,
        SyncAction::Push | SyncAction::PushLease | SyncAction::Publish | SyncAction::Sync
    ) {
        if action == SyncAction::PushLease && p.remote_oid.is_empty() {
            return Err("Fetch and configure an upstream before pushing with lease.".into());
        }
        output.push_str(&network(root, &push_args(&p, action)).await?);
    }
    Ok(GitResponse::Done(output))
}

async fn git_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let p = git(root, &["rev-parse", "--git-path", name]).await?;
    let p = PathBuf::from(p.trim());
    Ok(if p.is_absolute() { p } else { root.join(p) })
}
async fn operation(root: &Path) -> Option<String> {
    for (name, op) in [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
    ] {
        if git_path(root, name).await.ok().is_some_and(|p| p.exists()) {
            return Some(op.into());
        }
    }
    None
}

fn safe_file(root: &Path, path: &str) -> Result<PathBuf, String> {
    if path.is_empty()
        || Path::new(path).is_absolute()
        || path
            .split(['/', '\\'])
            .any(|c| c == ".." || c.eq_ignore_ascii_case(".git"))
    {
        return Err("Unsafe conflict path".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let file = root.join(path);
    let parent = file
        .parent()
        .ok_or("Missing parent")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !parent.starts_with(&root)
        || std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Unsafe conflict path".into());
    }
    Ok(file)
}
fn read_text(path: &Path) -> Result<String, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_TEXT {
        return Err("Conflict exceeds the 2 MiB text limit".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if text.contains('\0') {
        return Err("Binary conflict requires an external editor".into());
    }
    Ok(text)
}

async fn conflict_file(root: &Path, path: &str) -> Result<ConflictFile, String> {
    let unmerged = git(root, &["ls-files", "-u", "--", path]).await?;
    if unmerged.is_empty() {
        return Err("This file is no longer conflicted.".into());
    }
    let original = read_text(&safe_file(root, path)?)?;
    let mut stages = Vec::new();
    for stage in 1..=3 {
        let spec = format!(":{stage}:{path}");
        // Missing stages are valid for add/delete conflicts.
        let text = match git(root, &["rev-parse", "--verify", &spec]).await {
            Ok(oid) => {
                let size = git(root, &["cat-file", "-s", oid.trim()]).await?;
                if size.trim().parse::<u64>().map_err(|e| e.to_string())? > MAX_TEXT {
                    return Err("Conflict exceeds the 2 MiB text limit".into());
                }
                git(root, &["show", &spec]).await?
            }
            Err(_) => String::new(),
        };
        if text.contains('\0') {
            return Err("Binary conflict requires an external editor".into());
        }
        stages.push(text);
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    for (i, text) in stages.iter().enumerate() {
        std::fs::write(temp.path().join(i.to_string()), text).map_err(|e| e.to_string())?;
    }
    let out = tokio::process::Command::new("git")
        .args([
            "merge-file",
            "-p",
            "--diff3",
            "--marker-size=32",
            "-L",
            "ours",
            "-L",
            "base",
            "-L",
            "theirs",
        ])
        .arg(temp.path().join("1"))
        .arg(temp.path().join("0"))
        .arg(temp.path().join("2"))
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.code().is_some_and(|c| (0..=127).contains(&c)) {
        return Err(String::from_utf8_lossy(&out.stderr).into());
    }
    Ok(ConflictFile {
        path: path.into(),
        base: stages[0].clone(),
        ours: stages[1].clone(),
        theirs: stages[2].clone(),
        original,
        can_delete: git(root, &["rev-parse", "--verify", &format!(":2:{path}")])
            .await
            .is_err()
            || git(root, &["rev-parse", "--verify", &format!(":3:{path}")])
                .await
                .is_err(),
        hunks: String::from_utf8(out.stdout).map_err(|e| e.to_string())?,
    })
}

pub async fn conflicts(root: &Path) -> Result<GitResponse, String> {
    let paths = git(root, &["diff", "--name-only", "--diff-filter=U", "-z"]).await?;
    let mut files = Vec::new();
    let mut total_bytes = 0;
    for path in paths.split('\0').filter(|p| !p.is_empty()).take(100) {
        let file = conflict_file(root, path).await?;
        let bytes = file.base.len()
            + file.ours.len()
            + file.theirs.len()
            + file.original.len()
            + file.hunks.len();
        if total_bytes + bytes > 16 * 1024 * 1024 {
            break;
        }
        total_bytes += bytes;
        files.push(file);
    }
    Ok(GitResponse::Conflicts {
        operation: operation(root).await,
        files,
    })
}
pub async fn resolve(
    root: &Path,
    file: ConflictFile,
    result: Option<String>,
) -> Result<GitResponse, String> {
    if result.as_ref().is_some_and(|r| r.len() as u64 > MAX_TEXT) {
        return Err("Conflict exceeds the 2 MiB text limit".into());
    }
    let actual = conflict_file(root, &file.path).await?;
    if actual != file {
        return Err("Conflict changed on disk. Reload before resolving.".into());
    }
    // Never follow a client-specified symlink; replace the regular file atomically.
    let path = safe_file(root, &file.path)?;
    let Some(result) = result else {
        if !actual.can_delete {
            return Err("Neither side deleted this file.".into());
        }
        return git(root, &["rm", "-f", "--", &file.path])
            .await
            .map(GitResponse::Done);
    };
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing parent")?)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    temp.write_all(result.as_bytes())
        .map_err(|e| e.to_string())?;
    temp.as_file()
        .set_permissions(
            std::fs::metadata(&path)
                .map_err(|e| e.to_string())?
                .permissions(),
        )
        .map_err(|e| e.to_string())?;
    temp.persist(&path).map_err(|e| e.to_string())?;
    git(root, &["add", "--", &file.path])
        .await
        .map(GitResponse::Done)
}
pub async fn finish(root: &Path, abort: bool) -> Result<GitResponse, String> {
    let op = operation(root).await.ok_or("No operation in progress")?;
    if !abort
        && !git(root, &["diff", "--name-only", "--diff-filter=U"])
            .await?
            .is_empty()
    {
        return Err("Resolve all conflicts before continuing.".into());
    }
    git(root, &[&op, if abort { "--abort" } else { "--continue" }])
        .await
        .map(GitResponse::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credential_free_arguments_and_explicit_lease() {
        for url in [
            "https://user:secret@host/repo",
            "https://token@host/repo",
            "ssh://u:secret@host/repo",
            "https://host/repo?token=secret",
        ] {
            assert!(!credential_free(url));
        }
        for url in [
            "git@host:repo",
            "ssh://git@host/repo",
            "https://host/repo",
            "/tmp/test.git",
        ] {
            assert!(credential_free(url));
        }
        assert!(!secure_transport("http://example.invalid/repo"));
        assert!(!secure_transport("git://example.invalid/repo"));
        assert!(!secure_transport("ext::sh -c touch% /tmp/pwned"));
        assert!(!secure_transport("fd::7"));
        assert!(secure_transport("git@example.invalid:org/repo.git"));
        assert!(secure_transport("../local/repo.git"));
        assert!(!secure_transport("http://127.0.0.1.example.invalid/repo"));
        for url in [
            "http://127.0.0.1:8080/repo",
            "http://[::1]:8080/repo",
            "ssh://git@example.invalid/repo",
            "https://example.invalid/repo",
            "/tmp/remote.git",
        ] {
            assert!(secure_transport(url));
        }
        let p = SyncPlan {
            repository: "/fixture".into(),
            remote_identity: "test".into(),
            head: "a".into(),
            status: String::new(),
            remote: "origin".into(),
            branch: "main".into(),
            target: "refs/heads/main".into(),
            remote_oid: "abc123".into(),
            ahead: 1,
            behind: 0,
            dirty: false,
        };
        assert_eq!(
            push_args(&p, SyncAction::PushLease),
            [
                "push",
                "--progress",
                "--force-with-lease=refs/heads/main:abc123",
                "origin",
                "HEAD:refs/heads/main"
            ]
        );
        assert!(!push_args(&p, SyncAction::Push)
            .iter()
            .any(|a| a.contains("force")));
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::{cli::run, conflict::ConflictDocument};
    use std::process::Command;
    fn g(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    fn commit(root: &Path, text: &str) {
        std::fs::write(root.join("note.txt"), text).unwrap();
        g(root, &["add", "."]);
        g(root, &["commit", "-qm", "change"]);
    }
    struct Fixture {
        _temp: tempfile::TempDir,
        local: PathBuf,
        other: PathBuf,
        remote: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let local = temp.path().join("local");
            let other = temp.path().join("other");
            let remote = temp.path().join("remote.git");
            std::fs::create_dir(&local).unwrap();
            g(
                temp.path(),
                &[
                    "init",
                    "--bare",
                    "-q",
                    "--initial-branch=main",
                    remote.to_str().unwrap(),
                ],
            );
            g(&local, &["init", "-q", "-b", "main"]);
            for key in ["user.name", "user.email"] {
                g(&local, &["config", key, "test@example.invalid"]);
            }
            commit(&local, "base\n");
            g(
                &local,
                &["remote", "add", "origin", remote.to_str().unwrap()],
            );
            g(&local, &["push", "-u", "origin", "main"]);
            g(
                temp.path(),
                &[
                    "clone",
                    "-q",
                    remote.to_str().unwrap(),
                    other.to_str().unwrap(),
                ],
            );
            for key in ["user.name", "user.email"] {
                g(&other, &["config", key, "test@example.invalid"]);
            }
            Self {
                _temp: temp,
                local,
                other,
                remote,
            }
        }
        async fn sync(&self, action: SyncAction) -> Result<GitResponse, String> {
            let GitResponse::SyncPlan(p) = run(
                &self.local,
                GitRequest::Sync {
                    action,
                    expected: None,
                },
            )
            .await?
            else {
                panic!()
            };
            run(
                &self.local,
                GitRequest::Sync {
                    action,
                    expected: Some(p),
                },
            )
            .await
        }
        fn remote_head(&self) -> String {
            g(&self.remote, &["rev-parse", "main"])
        }
    }
    #[tokio::test]
    async fn fetch_pull_publish_rejected_push_and_stale_lease() {
        let f = Fixture::new();
        commit(&f.other, "remote\n");
        g(&f.other, &["push"]);
        run(
            &f.local,
            GitRequest::Sync {
                action: SyncAction::Fetch,
                expected: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(plan(&f.local).await.unwrap().behind, 1);
        f.sync(SyncAction::Pull).await.unwrap();
        assert_eq!(g(&f.local, &["rev-parse", "HEAD"]), f.remote_head());
        commit(&f.local, "local\n");
        f.sync(SyncAction::Push).await.unwrap();
        g(&f.other, &["pull", "--ff-only"]);
        commit(&f.other, "someone else\n");
        g(&f.other, &["push"]);
        commit(&f.local, "diverged\n");
        assert!(f
            .sync(SyncAction::Push)
            .await
            .unwrap_err()
            .contains("rejected"));
        assert!(f
            .sync(SyncAction::PushLease)
            .await
            .unwrap_err()
            .contains("stale info"));
        let GitResponse::SyncPlan(review) = run(
            &f.local,
            GitRequest::Sync {
                action: SyncAction::Pull,
                expected: None,
            },
        )
        .await
        .unwrap() else {
            panic!()
        };
        assert_eq!(
            (review.ahead, review.behind),
            (1, 1),
            "Pull reviews fresh divergence"
        );
        g(&f.local, &["checkout", "-qb", "published"]);
        f.sync(SyncAction::Publish).await.unwrap();
        assert_eq!(
            g(&f.local, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/published"
        );
    }
    #[tokio::test]
    async fn merge_rebase_and_cherry_pick_resolve_continue_and_abort() {
        for op in ["merge", "rebase", "cherry-pick"] {
            let f = Fixture::new();
            commit(&f.local, "ours\n");
            let before = g(&f.local, &["rev-parse", "HEAD"]);
            commit(&f.other, "theirs\n");
            g(&f.other, &["push"]);
            run(
                &f.local,
                GitRequest::Sync {
                    action: SyncAction::Fetch,
                    expected: None,
                },
            )
            .await
            .unwrap();
            let start = || {
                let args = if op == "cherry-pick" {
                    vec!["cherry-pick", "origin/main"]
                } else if op == "rebase" {
                    vec!["rebase", "origin/main"]
                } else {
                    vec!["merge", "--no-edit", "origin/main"]
                };
                let out = Command::new("git")
                    .arg("-C")
                    .arg(&f.local)
                    .args(args)
                    .output()
                    .unwrap();
                assert!(!out.status.success());
            };
            start();
            assert_eq!(operation(&f.local).await.as_deref(), Some(op));
            run(&f.local, GitRequest::Abort).await.unwrap();
            assert_eq!(g(&f.local, &["rev-parse", "HEAD"]), before);
            assert_eq!(
                std::fs::read_to_string(f.local.join("note.txt")).unwrap(),
                "ours\n"
            );
            assert!(g(&f.local, &["status", "--porcelain"]).is_empty());
            start();
            let GitResponse::Conflicts { files, .. } =
                run(&f.local, GitRequest::Conflicts).await.unwrap()
            else {
                panic!()
            };
            assert_eq!(files.len(), 1);
            let file = files[0].clone();
            let doc = ConflictDocument::parse(&file.hunks).unwrap();
            assert_eq!(doc.hunks.len(), 1);
            let result = doc.pick(&doc.result, 0, 2);
            // External writes cause a refusal, never overwrite them.
            std::fs::write(f.local.join("note.txt"), "external edit\n").unwrap();
            assert!(run(
                &f.local,
                GitRequest::Resolve {
                    file: file.clone(),
                    result: result.clone()
                }
            )
            .await
            .is_err());
            std::fs::write(f.local.join("note.txt"), &file.original).unwrap();
            run(&f.local, GitRequest::Resolve { file, result })
                .await
                .unwrap();
            run(&f.local, GitRequest::Continue).await.unwrap();
            assert!(operation(&f.local).await.is_none());
            if op == "rebase" || op == "merge" {
                f.sync(SyncAction::Push).await.unwrap();
                assert_eq!(g(&f.local, &["rev-parse", "HEAD"]), f.remote_head());
            }
        }
    }
    #[tokio::test]
    async fn deletion_is_distinct_from_empty_text() {
        let f = Fixture::new();
        g(&f.local, &["rm", "note.txt"]);
        g(&f.local, &["commit", "-qm", "delete"]);
        commit(&f.other, "remote change\n");
        g(&f.other, &["push"]);
        run(
            &f.local,
            GitRequest::Sync {
                action: SyncAction::Fetch,
                expected: None,
            },
        )
        .await
        .unwrap();
        let out = Command::new("git")
            .arg("-C")
            .arg(&f.local)
            .args(["merge", "origin/main"])
            .output()
            .unwrap();
        assert!(!out.status.success());
        let GitResponse::Conflicts { files, .. } =
            run(&f.local, GitRequest::Conflicts).await.unwrap()
        else {
            panic!()
        };
        assert!(files[0].can_delete);
        run(
            &f.local,
            GitRequest::ResolveDeletion {
                file: files[0].clone(),
            },
        )
        .await
        .unwrap();
        assert!(!f.local.join("note.txt").exists());
        run(&f.local, GitRequest::Continue).await.unwrap();
        f.sync(SyncAction::Push).await.unwrap();
        assert_eq!(g(&f.local, &["rev-parse", "HEAD"]), f.remote_head());
    }
    #[tokio::test]
    async fn dirty_and_stale_preconditions_refused() {
        let f = Fixture::new();
        std::fs::write(f.local.join("note.txt"), "dirty\n").unwrap();
        assert!(f
            .sync(SyncAction::Pull)
            .await
            .unwrap_err()
            .contains("dirty"));
        let expected = plan(&f.local).await.unwrap();
        commit(&f.local, "changed\n");
        assert!(run(
            &f.local,
            GitRequest::Sync {
                action: SyncAction::Push,
                expected: Some(expected)
            }
        )
        .await
        .unwrap_err()
        .contains("changed"));
        let expected = plan(&f.local).await.unwrap();
        g(
            &f.local,
            &[
                "remote",
                "set-url",
                "origin",
                "/tmp/a-different-local-remote.git",
            ],
        );
        assert!(run(
            &f.local,
            GitRequest::Sync {
                action: SyncAction::Push,
                expected: Some(expected)
            }
        )
        .await
        .unwrap_err()
        .contains("changed"));
        g(
            &f.local,
            &[
                "remote",
                "set-url",
                "origin",
                "https://token:secret@example.invalid/repo",
            ],
        );
        let err = f.sync(SyncAction::Push).await.unwrap_err();
        assert!(!err.contains("token:secret"));
        assert!(err.contains("credential"));
    }
}
