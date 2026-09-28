//! Milestone 16: `FolderSource::changes_since` follows changes made on disk
//! behind Moonkale's back, and only the ones the Explorer would show.

use moonkale_core::{Changes, Source};
use moonkale_project_fs::FolderSource;
use std::collections::BTreeSet;
use std::fs;
use std::time::Duration;

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join("target/debug")).unwrap();
    fs::write(p.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(p.join("README.md"), "# hi\n").unwrap();
    fs::write(p.join(".gitignore"), "target/\n*.log\n").unwrap();
    dir
}

/// Poll until every path in `want` has been reported (or give up).
async fn collect(src: &FolderSource, mut since: u64, want: &[&str]) -> (u64, BTreeSet<String>) {
    let mut got = BTreeSet::new();
    for _ in 0..40 {
        let Changes { seq, paths, reset } = src.changes_since(since).await.unwrap().unwrap();
        assert!(!reset, "no reset expected");
        since = seq;
        got.extend(paths);
        if want.iter().all(|w| got.contains(*w)) {
            break;
        }
    }
    (since, got)
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_creates_edits_deletes_and_new_directories() {
    let dir = fixture();
    let p = dir.path().to_path_buf();
    let src = FolderSource::open(&p).unwrap();
    // `0` = "from now": the current position, nothing listed.
    let start = src
        .changes_since(0)
        .await
        .unwrap()
        .expect("a folder is watched");
    assert!(start.paths.is_empty() && !start.reset);

    fs::write(p.join("NEW.md"), "new\n").unwrap();
    fs::write(p.join("src/main.rs"), "fn main() { println!(); }\n").unwrap();
    fs::remove_file(p.join("README.md")).unwrap();
    let (seq, got) = collect(&src, start.seq, &["NEW.md", "src/main.rs", "README.md"]).await;
    for want in ["NEW.md", "src/main.rs", "README.md"] {
        assert!(got.contains(want), "{want} missing from {got:?}");
    }

    // A new directory is reported, and then watched: a file created in it
    // afterwards is reported too.
    fs::create_dir(p.join("notes")).unwrap();
    let (seq, got) = collect(&src, seq, &["notes"]).await;
    assert!(got.contains("notes"), "{got:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    fs::write(p.join("notes/idea.md"), "x\n").unwrap();
    let (_, got) = collect(&src, seq, &["notes/idea.md"]).await;
    assert!(got.contains("notes/idea.md"), "{got:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ignores_hidden_gitignored_and_temporary_files() {
    let dir = fixture();
    let p = dir.path().to_path_buf();
    let src = FolderSource::open(&p).unwrap();
    let start = src.changes_since(0).await.unwrap().unwrap();

    fs::write(p.join("target/debug/junk"), "x").unwrap(); // not even watched
    fs::write(p.join("build.log"), "x").unwrap(); // .gitignore: *.log
    fs::write(p.join(".hidden"), "x").unwrap();
    fs::create_dir_all(p.join(".moonkale")).unwrap();
    fs::write(p.join("README.md.moonkale-tmp"), "x").unwrap();
    // …and one change that does count, as the end marker.
    fs::write(p.join("seen.md"), "x").unwrap();

    let (_, got) = collect(&src, start.seq, &["seen.md"]).await;
    assert_eq!(got.into_iter().collect::<Vec<_>>(), ["seen.md"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_position_asks_for_a_reset_and_an_idle_poll_returns_empty() {
    let dir = fixture();
    let src = FolderSource::open(dir.path()).unwrap();
    let start = src.changes_since(0).await.unwrap().unwrap();
    // A position from an earlier watcher (a restarted server) is not in this log.
    let stale = src.changes_since(1).await.unwrap().unwrap();
    assert!(stale.reset && stale.seq == start.seq);
    // Nothing happens: the poll comes back with the same position and no
    // paths (after its wait — bounded, checked here with a timeout).
    let idle = tokio::time::timeout(Duration::from_secs(40), src.changes_since(start.seq))
        .await
        .expect("the long poll ends by itself")
        .unwrap()
        .unwrap();
    assert_eq!(
        (idle.seq, idle.paths.len(), idle.reset),
        (start.seq, 0, false)
    );
}
