//! #5: the folder source never reads or writes outside its folder.
#![cfg(unix)]

use moonkale_core::{OpResult, Query, Source, SourceError, Transaction};
use moonkale_project_fs::{tree, FolderSource};
use std::fs;
use std::os::unix::fs::symlink;

async fn by_path(src: &FolderSource, path: &str) -> Result<moonkale_core::NodeId, SourceError> {
    src.query(Query::Text {
        dialect: "path".into(),
        text: path.into(),
    })
    .await
    .map(|r| r.nodes[0].id)
}

#[test]
fn only_plain_relative_paths() {
    for ok in [
        "",
        "a.md",
        "notes/a.md",
        "./a.md",
        ".moonkale/settings.json",
    ] {
        assert!(tree::check_rel(ok).is_ok(), "{ok}");
    }
    for bad in [
        "../x",
        "a/../../x",
        "/etc/passwd",
        "a\\..\\..\\x",
        "C:\\Users\\x",
        "a\0b",
    ] {
        assert!(tree::check_rel(bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn links_leading_outside_are_not_followed() {
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "hunter2\n").unwrap();
    fs::create_dir(outside.path().join("dir")).unwrap();
    fs::write(outside.path().join("dir/inner.txt"), "inner\n").unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("real.md"), "inside\n").unwrap();
    symlink(
        outside.path().join("secret.txt"),
        dir.path().join("leak.md"),
    )
    .unwrap();
    symlink(outside.path().join("dir"), dir.path().join("linked")).unwrap();
    symlink(dir.path().join("real.md"), dir.path().join("alias.md")).unwrap();
    symlink(
        outside.path().join("nowhere.txt"),
        dir.path().join("dangling.md"),
    )
    .unwrap();
    let src = FolderSource::open(dir.path()).unwrap();

    // A file link to outside: listed, but neither stat'ed nor read.
    assert!(by_path(&src, "leak.md").await.is_err());
    // Through the tree: the Children listing shows it; fetching it fails.
    let root = src.query(Query::Children(src.root_id())).await.unwrap();
    let leak = root
        .nodes
        .iter()
        .find(|n| n.label == "leak.md")
        .map(|n| n.id)
        .expect("the link is listed");
    let r = src.fetch_text(leak).await;
    assert!(r.is_err(), "{r:?}");
    // A directory link to outside: not listed into.
    let linked = root
        .nodes
        .iter()
        .find(|n| n.label == "linked")
        .map(|n| n.id)
        .expect("the directory link is listed");
    let r = src.query(Query::Children(linked)).await;
    assert!(r.is_err(), "{r:?}");
    assert!(by_path(&src, "linked/inner.txt").await.is_err());
    let ls = src
        .query(Query::Text {
            dialect: "ls".into(),
            text: "linked".into(),
        })
        .await;
    assert!(ls.map(|r| r.nodes.is_empty()).unwrap_or(true));

    // A link within the folder is fine.
    let alias = by_path(&src, "alias.md").await.unwrap();
    assert_eq!(src.fetch_text(alias).await.unwrap().0, "inside\n");

    // Creating through a link (dangling or outside) never writes outside.
    let r = src
        .apply(Transaction::create_text(
            src.root_id(),
            "linked/new.md",
            "x",
        ))
        .await
        .unwrap();
    assert!(
        matches!(r.results[0], OpResult::Refused { .. }),
        "{:?}",
        r.results[0]
    );
    assert!(!outside.path().join("dir/new.md").exists());
    let r = src
        .apply(Transaction::create_text(src.root_id(), "dangling.md", "x"))
        .await
        .unwrap();
    assert!(matches!(r.results[0], OpResult::Refused { .. }));
    assert!(!outside.path().join("nowhere.txt").exists());

    // But a bad link can be removed: delete moves the link, not its target.
    let r = src.apply(Transaction::delete(leak)).await.unwrap();
    assert!(
        matches!(r.results[0], OpResult::Ok { .. }),
        "{:?}",
        r.results[0]
    );
    assert!(fs::symlink_metadata(dir.path().join("leak.md")).is_err());
    assert_eq!(
        fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "hunter2\n"
    );
}
