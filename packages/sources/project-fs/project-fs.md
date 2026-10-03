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
Web/mobile backends (`platform.rs` note), the `Backend` trait split — not before a second backend exists.

## Tests
`cargo test -p moonkale-project-fs` — 5 integration tests in `tests/folder_source.rs` on a tempdir fixture with a `.gitignore`, a binary file and a nested dir: listing order + ignore, stable ids across re-open, write round trip + atomicity, stale version → `Conflict` and disk untouched, unknown id → `NotFound`.

## Milestone 4
`FolderSource::apply` handles `Op::CreateText`: rejects `..`, creates parent directories, refuses an existing path, registers the new node's id. Test in `tests/folder_source.rs`.

## Milestone 7
`Op::CreateDir`, `Op::Rename { node, to }` (new id from the new path, old id forgotten; refuses clashes, escapes and moving a directory into itself), `Op::Delete` (moves to `.moonkale/trash/<unix-ms>/<path>`; the root cannot be deleted). Test `create_dir_rename_and_delete_to_trash`.

## Milestone 15 — the `ls` dialect
`Query::Text { dialect: "ls", text: "<rel>" }` lists a directory by relative path with a plain `read_dir`: hidden entries and ignored ones included, sorted by name, root-jailed (`..` refused), empty for a missing directory. `Children` keeps the explorer's rules (hidden and `.gitignore`d entries skipped), which is why `.moonkale/agent-sessions/local/` needed this (P-118).

## Milestone 16 — watching
`watch.rs`: `FolderWatch` (native only, `notify` 8). One **non-recursive** watch per directory the `.gitignore`-aware walk visits — never `target/`, `node_modules/`, `.git/` (a Rust workspace's `target/` alone would exhaust the inotify limit); directories created later are added as they appear. Events for hidden paths, paths the root `.gitignore` excludes, `*.moonkale-tmp` and access events are dropped (a file ignored only by a nested `.gitignore` costs one harmless extra refresh). A thread keeps a bounded log (4 096 entries) of changed relative paths under increasing positions; the base is the start time, so a position from an earlier watcher answers `reset`. `changes_since(since, wait)`: `0` → the current position at once; otherwise waits up to `wait` (25 s from `FolderSource`) for the first change, 120 ms more for the rest of the burst, then the distinct paths — more than 512 become `reset`. `FolderSource::changes_since` starts the watcher on first use (`spawn_blocking`) and reports `None` if it cannot start. Tests: `tests/watch.rs` (create/edit/delete, a new directory and a file in it, hidden/ignored/temporary files dropped, stale position → reset, idle poll ends).
