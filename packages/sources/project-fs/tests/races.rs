//! #15: file operations that lost or misdirected data.

use moonkale_core::{OpResult, Query, Source, SourceError, TextPatch, Transaction};
use moonkale_project_fs::FolderSource;
use std::fs;

fn ok(a: &moonkale_core::Applied) -> moonkale_core::NodeId {
    match &a.results[0] {
        OpResult::Ok { node, .. } => *node,
        other => panic!("{other:?}"),
    }
}

async fn by_path(src: &FolderSource, path: &str) -> moonkale_core::NodeId {
    let r = src
        .query(Query::Text {
            dialect: "path".into(),
            text: path.into(),
        })
        .await
        .unwrap();
    r.nodes[0].id
}

#[tokio::test]
async fn renaming_a_directory_forgets_the_ids_under_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let docs = ok(&src
        .apply(Transaction::create_dir(src.root_id(), "docs"))
        .await
        .unwrap());
    let note = ok(&src
        .apply(Transaction::create_text(docs, "note.md", "mine\n"))
        .await
        .unwrap());
    ok(&src
        .apply(Transaction::rename(docs, "archive"))
        .await
        .unwrap());
    // Someone makes an unrelated file at the old path.
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/note.md"), "someone else's\n").unwrap();
    // The old id must not read (or write) it.
    assert!(matches!(
        src.fetch_text(note).await,
        Err(SourceError::NotFound)
    ));
    // The moved file is still there under its new path.
    let moved = by_path(&src, "archive/note.md").await;
    assert_eq!(src.fetch_text(moved).await.unwrap().0, "mine\n");
}

#[tokio::test]
async fn deleting_a_directory_forgets_the_ids_under_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let docs = ok(&src
        .apply(Transaction::create_dir(src.root_id(), "docs"))
        .await
        .unwrap());
    let note = ok(&src
        .apply(Transaction::create_text(docs, "note.md", "x\n"))
        .await
        .unwrap());
    ok(&src.apply(Transaction::delete(docs)).await.unwrap());
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/note.md"), "new\n").unwrap();
    assert!(matches!(
        src.fetch_text(note).await,
        Err(SourceError::NotFound)
    ));
}

#[tokio::test]
async fn deletes_never_overwrite_an_earlier_trash_copy_and_old_trash_is_pruned() {
    let dir = tempfile::tempdir().unwrap();
    // A trash folder from long ago.
    let old = dir.path().join(".moonkale/trash/1000");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("gone.md"), "old\n").unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    // Delete, recreate, delete — fast, often within one millisecond.
    for text in ["first\n", "second\n", "third\n"] {
        fs::write(dir.path().join("a.md"), text).unwrap();
        let id = by_path(&src, "a.md").await;
        ok(&src.apply(Transaction::delete(id)).await.unwrap());
    }
    let mut copies: Vec<String> = fs::read_dir(dir.path().join(".moonkale/trash"))
        .unwrap()
        .filter_map(|e| fs::read_to_string(e.unwrap().path().join("a.md")).ok())
        .collect();
    copies.sort();
    assert_eq!(copies, ["first\n", "second\n", "third\n"]);
    assert!(!old.exists(), "trash older than 30 days is pruned");
}

#[tokio::test]
async fn saving_uses_a_temp_file_of_its_own_and_leaves_none_behind() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "one\n").unwrap();
    // A real file with the name the old scheme used for its temp file.
    fs::write(dir.path().join("a.md.moonkale-tmp"), "keep me\n").unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let id = by_path(&src, "a.md").await;
    let (text, v) = src.fetch_text(id).await.unwrap();
    let applied = src
        .apply(Transaction::write_text(
            id,
            v,
            TextPatch::whole("two\n", text.chars().count()),
        ))
        .await
        .unwrap();
    ok(&applied);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "two\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("a.md.moonkale-tmp")).unwrap(),
        "keep me\n"
    );
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.') && n.ends_with(".moonkale-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn create_refuses_a_file_that_exists_without_touching_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    fs::write(dir.path().join("taken.md"), "theirs\n").unwrap();
    let r = src
        .apply(Transaction::create_text(
            src.root_id(),
            "taken.md",
            "mine\n",
        ))
        .await
        .unwrap();
    assert!(matches!(r.results[0], OpResult::Refused { .. }));
    assert_eq!(
        fs::read_to_string(dir.path().join("taken.md")).unwrap(),
        "theirs\n"
    );
}

/// #14: a file past the fetch limit is refused, not read into memory.
#[tokio::test]
async fn huge_files_are_not_read_whole() {
    let dir = tempfile::tempdir().unwrap();
    let f = fs::File::create(dir.path().join("big.log")).unwrap();
    f.set_len(moonkale_project_fs::source::MAX_FETCH + 1)
        .unwrap(); // sparse
    let src = FolderSource::open(dir.path()).unwrap();
    let id = by_path(&src, "big.log").await;
    assert!(matches!(
        src.fetch_text(id).await,
        Err(SourceError::Unsupported(_))
    ));
    assert!(matches!(
        src.fetch_bytes(id).await,
        Err(SourceError::Unsupported(_))
    ));
}
