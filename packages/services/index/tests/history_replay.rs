#![cfg(all(feature = "sophia", not(target_arch = "wasm32")))]
mod replay_support;
use replay_support::git;
use std::{fs, path::Path};
fn commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]).unwrap();
    git(
        root,
        &[
            "-c",
            "user.name=Replay",
            "-c",
            "user.email=replay@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-m",
            message,
        ],
    )
    .unwrap();
}
#[tokio::test]
async fn five_commit_history_matches_fresh_stores() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    git(root, &["init"]).unwrap();
    fs::write(
        root.join("a.jl"),
        "const answer = 1\nf(x::Int) = x\nf(x::Float64) = x\ncaller(x) = f(x)\n",
    )
    .unwrap();
    commit(root, "add");
    fs::write(
        root.join("a.jl"),
        "const answer = 2\nf(x::Int) = x\nf(x::Float64) = x\ncaller(x) = f(x)\n",
    )
    .unwrap();
    commit(root, "modify");
    fs::rename(root.join("a.jl"), root.join("renamed.jl")).unwrap();
    commit(root, "rename");
    fs::write(
        root.join("renamed.jl"),
        "f(x::Int) = x\nf(x::Float64) = x\ncaller(x) = f(x)\n",
    )
    .unwrap();
    fs::write(root.join("other.jl"), "const answer = 2\n").unwrap();
    commit(root, "move definition");
    fs::remove_file(root.join("other.jl")).unwrap();
    commit(root, "delete");
    let before = git(root, &["status", "--porcelain=v1", "--untracked-files=all"]).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let mut output = Vec::new();
    let result = replay_support::replay(root, "HEAD", scratch.path(), 1, &mut output)
        .await
        .unwrap();
    assert_eq!(result.commits, 5);
    assert_eq!(
        (result.findings, result.errors),
        (0, 0),
        "{}",
        String::from_utf8_lossy(&output)
    );
    assert_eq!(
        git(root, &["status", "--porcelain=v1", "--untracked-files=all"]).unwrap(),
        before
    );
    // Refuse a caller-provided scratch location inside the input repo before
    // creating a cache or any other file there.
    assert!(replay_support::replay(root, "HEAD", root, 1, &mut output)
        .await
        .is_err());
    assert!(!root.join(".moonkale").exists());
    // A non-root range must seed its parent, too.
    let result = replay_support::replay(root, "HEAD~2..HEAD", scratch.path(), 1, &mut output)
        .await
        .unwrap();
    assert_eq!((result.commits, result.findings, result.errors), (2, 0, 0));
    // SPEC-010 acceptance 2: the truth moves into the store after the
    // rename; the last two commits are appended as history.
    let mut output = Vec::new();
    let result = replay_support::replay_with(root, "HEAD", scratch.path(), 1, Some(3), &mut output)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output);
    assert_eq!(
        (result.commits, result.findings, result.errors),
        (5, 0, 0),
        "{text}"
    );
    assert!(text.contains(r#""type":"switch""#) && text.contains(r#""type":"give_back""#));
    assert!(text.contains(r#""twin_compared":true"#), "{text}");
}

/// P-231: Sophia v0.1.0 subtracted duplicate old generic member ids twice
/// while counting the new contribution once (fixed in Sophia 77f5c1c9).
#[tokio::test]
async fn repeated_generic_content_survives_a_file_refresh() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    git(root, &["init"]).unwrap();
    fs::write(root.join("a.jl"), "f(x::Int) = x\nf(y::Int) = y\n").unwrap();
    commit(root, "two methods with the same normalized identity");
    fs::write(
        root.join("a.jl"),
        "# refresh\nf(x::Int) = x\nf(y::Int) = y\n",
    )
    .unwrap();
    commit(root, "refresh without changing methods");
    let scratch = tempfile::tempdir().unwrap();
    let mut output = Vec::new();
    let result = replay_support::replay(root, "HEAD", scratch.path(), 1, &mut output)
        .await
        .unwrap();
    assert_eq!(
        (result.findings, result.errors),
        (0, 0),
        "{}",
        String::from_utf8_lossy(&output)
    );
}
