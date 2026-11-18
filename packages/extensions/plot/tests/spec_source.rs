//! A `.plot.json` source path opens only what lies inside the folder
//! (review finding 2026-11-18: a link inside the folder led to an outside
//! database, which the spec then opened; re-review: a path the jail
//! normalized differently from the one the panel opened).
#![cfg(unix)]
use moonkale_ext_plot::spec_source_inside;
use moonkale_project_fs::FolderSource;
use std::fs;
use std::os::unix::fs::symlink;

#[tokio::test]
async fn spec_sources_stay_inside_the_folder() {
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("outside.sqlite"), b"not ours").unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("data")).unwrap();
    fs::write(dir.path().join("data/inside.sqlite"), b"ours").unwrap();
    symlink(
        outside.path().join("outside.sqlite"),
        dir.path().join("linked.sqlite"),
    )
    .unwrap();
    symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
    symlink(
        dir.path().join("data/inside.sqlite"),
        dir.path().join("alias.sqlite"),
    )
    .unwrap();
    // A link whose name only differs by whitespace from an inside file.
    fs::write(dir.path().join("inside.sqlite"), b"ours").unwrap();
    symlink(
        outside.path().join("outside.sqlite"),
        dir.path().join(" inside.sqlite"),
    )
    .unwrap();
    let src = FolderSource::open(dir.path()).unwrap();

    let ok = |p: &str| Some(p.to_string());
    assert_eq!(
        spec_source_inside(&src, "data/inside.sqlite").await,
        ok("data/inside.sqlite")
    );
    assert_eq!(
        spec_source_inside(&src, "/data/inside.sqlite").await,
        ok("data/inside.sqlite")
    );
    assert_eq!(
        spec_source_inside(&src, "./alias.sqlite").await,
        ok("alias.sqlite")
    );
    // The returned path is the one checked and the one to open: never the
    // outside link " inside.sqlite".
    assert_eq!(
        spec_source_inside(&src, " inside.sqlite").await,
        ok("inside.sqlite")
    );
    for refused in [
        "linked.sqlite",
        "linked-dir/outside.sqlite",
        "../outside.sqlite",
        "data/../../outside.sqlite",
        "missing.sqlite",
        "",
        " / ",
    ] {
        assert_eq!(spec_source_inside(&src, refused).await, None, "{refused:?}");
    }
    let abs = outside.path().join("outside.sqlite");
    assert_eq!(spec_source_inside(&src, abs.to_str().unwrap()).await, None);
}
