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

#[tokio::test]
async fn equal_length_equal_mtime_changes_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    fs::write(&path, "old").unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    let modified = file.metadata().unwrap().modified().unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let id = by_path(&src, "a.md").await;
    let (_, version) = src.fetch_text(id).await.unwrap();
    fs::write(&path, "new").unwrap();
    file.set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let result = src
        .apply(Transaction::write_text(
            id,
            version,
            TextPatch::whole("bad", 3),
        ))
        .await
        .unwrap();
    assert!(matches!(
        result.results[0],
        OpResult::Refused {
            error: SourceError::Conflict { .. },
            ..
        }
    ));
    assert_eq!(fs::read_to_string(path).unwrap(), "new");
}

#[tokio::test]
async fn directory_versions_agree_across_queries() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let children = src.query(Query::Children(src.root_id())).await.unwrap();
    let ls = src
        .query(Query::Text {
            dialect: "ls".into(),
            text: "".into(),
        })
        .await
        .unwrap();
    assert_eq!(children.nodes[0].version, ls.nodes[0].version);
    let direct = src.query(Query::Node(children.nodes[0].id)).await.unwrap();
    assert_eq!(direct.nodes[0].version, children.nodes[0].version);
}

#[tokio::test]
async fn neighbourhood_obeys_depth_direction_and_contains_edges() {
    use moonkale_core::Direction;
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/a.md"), "a").unwrap();
    fs::write(dir.path().join("sibling.md"), "b").unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let leaf = by_path(&src, "docs/a.md").await;
    for (node, depth, direction, count, edges) in [
        (src.root_id(), 0, Direction::Both, 1, 0),
        (src.root_id(), 1, Direction::Out, 3, 2),
        (src.root_id(), 2, Direction::Out, 4, 3),
        (leaf, 2, Direction::In, 3, 2),
        (leaf, 1, Direction::Out, 1, 0),
        (leaf, 2, Direction::Both, 3, 2),
    ] {
        let r = src
            .query(Query::Neighbours {
                node,
                depth,
                direction,
            })
            .await
            .unwrap();
        assert_eq!(r.nodes.len(), count, "{depth} {direction:?}");
        assert_eq!(r.edges.len(), edges);
        assert!(!r.truncated);
    }
}

#[tokio::test]
async fn simultaneous_saves_from_separate_sources_have_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "old").unwrap();
    let a = FolderSource::open(dir.path()).unwrap();
    let b = FolderSource::open(dir.path()).unwrap();
    let id = by_path(&a, "a.md").await;
    by_path(&b, "a.md").await;
    let (_, version) = a.fetch_text(id).await.unwrap();
    let ta = Transaction::write_text(id, version, TextPatch::whole("one", 3));
    let tb = Transaction::write_text(id, version, TextPatch::whole("two", 3));
    let (ra, rb) = tokio::join!(a.apply(ta), b.apply(tb));
    let wins = [ra.unwrap(), rb.unwrap()]
        .iter()
        .filter(|r| matches!(r.results[0], OpResult::Ok { .. }))
        .count();
    assert_eq!(wins, 1);
}

#[tokio::test]
async fn rename_refuses_existing_destination_and_preserves_both() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "a").unwrap();
    fs::write(dir.path().join("b.md"), "b").unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let id = by_path(&src, "a.md").await;
    let r = src.apply(Transaction::rename(id, "b.md")).await.unwrap();
    assert!(matches!(r.results[0], OpResult::Refused { .. }));
    assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), "a");
    assert_eq!(fs::read_to_string(dir.path().join("b.md")).unwrap(), "b");
}

#[tokio::test]
async fn neighbourhood_caps_deep_and_wide_trees() {
    use moonkale_core::Direction;
    let dir = tempfile::tempdir().unwrap();
    let mut path = dir.path().to_path_buf();
    for _ in 0..66 {
        path.push("d");
        fs::create_dir(&path).unwrap();
    }
    let src = FolderSource::open(dir.path()).unwrap();
    let r = src
        .query(Query::Neighbours {
            node: src.root_id(),
            depth: u32::MAX,
            direction: Direction::Out,
        })
        .await
        .unwrap();
    assert_eq!(r.nodes.len(), 65);
    assert!(r.truncated);
    // Avoid a second large tree: populate the deepest directory with siblings.
    for i in 0..4100 {
        fs::write(path.join(format!("{i}.md")), "").unwrap();
    }
    let id = by_path(&src, &vec!["d"; 66].join("/")).await;
    let r = src
        .query(Query::Neighbours {
            node: id,
            depth: 1,
            direction: Direction::Out,
        })
        .await
        .unwrap();
    assert_eq!(r.nodes.len(), 4096);
    assert!(r.truncated);
}

// rustix is a dependency only on Linux/Android; Windows has no POSIX FIFOs.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[tokio::test]
async fn listing_a_fifo_does_not_open_it_for_hashing() {
    use rustix::fs::{mkfifoat, Mode, CWD};
    let dir = tempfile::tempdir().unwrap();
    mkfifoat(CWD, dir.path().join("pipe"), Mode::RUSR | Mode::WUSR).unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let r = src.query(Query::Children(src.root_id())).await.unwrap();
    assert_eq!(r.nodes.len(), 1);
    assert!(matches!(
        src.fetch_text(r.nodes[0].id).await,
        Err(SourceError::Unsupported(_))
    ));
    assert!(matches!(
        src.fetch_bytes(r.nodes[0].id).await,
        Err(SourceError::Unsupported(_))
    ));
}

#[tokio::test]
async fn save_preconditions_come_from_content_not_listing_hints() {
    let dir = tempfile::tempdir().unwrap();
    let src = FolderSource::open(dir.path()).unwrap();
    let created = src
        .apply(Transaction::create_text(src.root_id(), "a.md", "old"))
        .await
        .unwrap();
    let id = ok(&created);
    let (text, version) = src.fetch_text(id).await.unwrap();
    assert_eq!(created.version_of(id), Some(version));
    let hint = src.query(Query::Node(id)).await.unwrap().nodes[0].version;
    assert_ne!(hint, version);
    let refused = src
        .apply(Transaction::write_text(
            id,
            hint,
            TextPatch::whole("bad", text.chars().count()),
        ))
        .await
        .unwrap();
    assert!(matches!(
        refused.results[0],
        OpResult::Refused {
            error: SourceError::Conflict { .. },
            ..
        }
    ));
    let saved = src
        .apply(Transaction::write_text(
            id,
            version,
            TextPatch::whole("new", text.chars().count()),
        ))
        .await
        .unwrap();
    ok(&saved);
    assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), "new");
}
