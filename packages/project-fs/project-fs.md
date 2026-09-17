---
title: "project-fs — implementation notes"
tags: [crate-notes, milestone-1]
---
Notes for `moonkale-project-fs` (Milestone 1). Design: [[Data Sources]], [[Platform Matrix]]. Log: [[Milestone 1 - Implementation Log]].

## Shape
- `tree.rs` — `list_children(root, dir)`: one level, via `ignore::WalkBuilder` rooted at the *folder root* (so parent `.gitignore`s apply) with `max_depth` and a `filter_entry` that prunes everything not on the path to `dir`. Sorted dirs-first.
- `source.rs` — `FolderSource`: `open(path)` canonicalises and builds `SourceId = "folder:<abs path>"`. Keeps a `RwLock<HashMap<NodeId, String>>` of ids → relative paths for everything it has listed; `fetch_text`/`apply` on an unknown id is `NotFound`.
- Only compiled on non-wasm targets; the crate is an empty shell on wasm32 so `cargo check --workspace` stays green.

## Critical decisions
- **`require_git(false)`** — honour `.gitignore` even outside a git repo (the `ignore` crate's default is the opposite). P-035.
- **Version = hash(mtime_nanos, len)**. Changes on any external write; conflicts are tested. Two writes within one mtime tick with equal length would collide — accepted for now; a content hash arrives with the index.
- **Atomic write**: patch applied in memory → written to `<file>.<ext>.moonkale-tmp` → `rename`. The E2E asserts the temp file is gone afterwards.
- **Lazy listing**, never a full walk: a monorepo costs one `read_dir` per expanded folder.
- **Binary detection is an extension blacklist** (`looks_textual`), and non-UTF-8 reads become `Unsupported`, so a `.png` is greyed in the tree and cannot be opened as text.
- `spawn_blocking` for the walk; `tokio::fs` for reads/writes.

## Not done
Watching (`watch.rs` note), web/mobile backends (`platform.rs` note), the `Backend` trait split — not before a second backend exists.

## Tests
`cargo test -p moonkale-project-fs` — 5 integration tests in `tests/folder_source.rs` on a tempdir fixture with a `.gitignore`, a binary file and a nested dir: listing order + ignore, stable ids across re-open, write round trip + atomicity, stale version → `Conflict` and disk untouched, unknown id → `NotFound`.
