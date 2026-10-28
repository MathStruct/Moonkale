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
- **Two version roles** (WP-23, reviewed 2026-10-17): query nodes (`Children`, `ls`, `Node`, `path`, `Neighbours`) carry a cheap metadata change hint: mtime ns + length, plus dev/inode/ctime on Unix. They never open file content. Hints are for refresh/cache invalidation, **not edit preconditions**. `fetch_text`/`fetch_bytes` return the first 64 bits of BLAKE3 of exactly the fetched bytes; `WriteText.expected` is checked only against this content version. Equal-length edits with unchanged mtime still conflict. Creating text returns a hash of the bytes just written. `Version(u64)` stays opaque and equality-only, not monotonic or a cryptographic identity. Fetch first before saving; unreadable existing text must be refused. Directory metadata hints agree across all query paths.
- **Atomic write**: lock one read/write handle with `File::try_lock` (refuse on contention/unsupported locking), read and validate that handle, apply the patch, stage a unique newly-created sibling, re-read the locked handle and check the pathname's file identity, then rename. The version returned hashes the submitted bytes. This excludes competing Moonkale saves, including separate source instances/processes, and refuses changes during staging. Unix advisory locks cannot exclude an unrelated writer ignoring the lock between the final checks and rename. A filesystem does not provide a portable conditional replacement primitive: **issue #15 remains open for that final external-writer window**, and root/parent symlink swaps remain #5. Failed saves remove their temporary file. Fetches and saves cap reads at 64 MiB + one sentinel byte even if a file grows during the read.
- **Lazy listing**, never a full walk: a monorepo costs one `read_dir` per expanded folder.
- **Binary detection is an extension blacklist** (`looks_textual`), and non-UTF-8 reads become `Unsupported`, so a `.png` is greyed in the tree and cannot be opened as text.
- `spawn_blocking` for the walk; `tokio::fs` for reads/writes.

## Not done
Web/mobile backends (`platform.rs` note), the `Backend` trait split — not before a second backend exists.

## Tests
`cargo test -p moonkale-project-fs` — integration suites in `tests/folder_source.rs`, `tests/races.rs`, `tests/jail.rs` and `tests/watch.rs`, plus internal staging/rename/hash tests. Tempdir fixtures cover `.gitignore`, binary and nested files: listing order, stable ids, write round trips, stale versions, unknown ids, jail boundaries and watcher lifecycle.

## Milestone 4
`FolderSource::apply` handles `Op::CreateText`: rejects `..`, creates parent directories, refuses an existing path, registers the new node's id. Test in `tests/folder_source.rs`.

## Milestone 7
`Op::CreateDir`, `Op::Rename { node, to }` (new id from the new path, old id forgotten; refuses clashes, escapes and moving a directory into itself), `Op::Delete` (moves to `.moonkale/trash/<unix-ms>/<path>`; the root cannot be deleted). Test `create_dir_rename_and_delete_to_trash`.

## Milestone 15 — the `ls` dialect
`Query::Text { dialect: "ls", text: "<rel>" }` lists a directory by relative path with a plain `read_dir`: hidden entries and ignored ones included, sorted by name, root-jailed (`..` refused), empty for a missing directory. `Children` keeps the explorer's rules (hidden and `.gitignore`d entries skipped), which is why `.moonkale/agent-sessions/local/` needed this (P-118).

## Milestone 16 — watching
`watch.rs`: `FolderWatch` (native only, `notify` 8). One **non-recursive** watch per directory the `.gitignore`-aware walk visits — never `target/`, `node_modules/`, `.git/` (a Rust workspace's `target/` alone would exhaust the inotify limit); directories created later are added as they appear. Events for hidden paths, paths the root `.gitignore` excludes, `*.moonkale-tmp` and access events are dropped (a file ignored only by a nested `.gitignore` costs one harmless extra refresh). A thread keeps a bounded log (4 096 entries) of changed relative paths under increasing positions; the base is the start time, so a position from an earlier watcher answers `reset`. `changes_since(since, wait)`: `0` → the current position at once; otherwise waits up to `wait` (25 s from `FolderSource`) for the first change, 120 ms more for the rest of the burst, then the distinct paths — more than 512 become `reset`. `FolderSource::changes_since` starts the watcher on first use (`spawn_blocking`) and reports `None` if it cannot start. Tests: `tests/watch.rs` (create/edit/delete, a new directory and a file in it, hidden/ignored/temporary files dropped, stale position → reset, idle poll ends).

## WP-23 — containment, rename and read diagnostics

`Neighbours` includes the origin and follows containment outward (children), inward (parent), or both using breadth-first traversal. Depth zero returns the origin only. It caps results at 4096 nodes and expansion at 64 hops; a cut reports `truncated`. Each directory expansion also bounds its collected entries to 4096 before sorting. Ignore/hidden rules apply to outward traversal; inward traversal follows the known pathname. Containment edges among reached nodes are included. Linked directory cycles cannot expand without hitting the caps.

Native moves use Linux/Android `renameat2(RENAME_NOREPLACE)`, macOS `renamex_np(RENAME_EXCL)`, or Windows `MoveFileExW` with zero flags (no replacement or cross-volume copy). If a native primitive is unsupported, the move refuses. Only other platforms use check-then-rename and retain a small race. Linux is tested locally; WP-26 adds advisory Windows/macOS CI tests, including native no-replace moves, saves and watchers. First native GitHub runs remain pending (neither Rust target can be installed in the local read-only rustup cache). Plain-path jail tests run on all three OSes; symlink fixtures stay Unix-only and the rustix FIFO fixture stays Linux/Android-only. See [[How to Write Tests]].

The index records a content-free `index_error = "text read failed"` property on mirrored nodes when UTF-8 reads fail. It removes prior search/derived data and clears the diagnostic after a successful read. It never extracts lossy text. This also reports other read failures without logging paths, contents or OS error messages. See `services/index/index.md`.

Regression tests in `tests/races.rs` cover unchanged mtime/length conflicts, simultaneous source saves, directory version agreement, direction/depth/caps, rename exclusion and rejection of listing hints as save preconditions. An internal 64 MiB sparse-file test counts content opens: `Children`, `ls`, `path`, `Node` and `Neighbours` all perform zero, while `fetch_bytes` increments the same counter and returns a content hash. Internal save tests inject edits/path replacement during staging and check refusal/temp cleanup. The obsolete streaming-listing hasher and timing test have been removed; listing cost no longer scales with bytes in each file.

Workspace state-file writes, file-backed history and folder settings all fetch text plus its content version before replacing existing files. A read failure refuses the write; there is no fallback to `node.version` or invented empty text. Document open/reload already store fetched versions, and proposals use those document versions. Image preview freshness compares query hints before/after fetching bytes rather than comparing a query hint with a content hash.

## Folder truth (ADR-0030, 2026-10-27)

`truth.rs` shares bounded `.moonkale/truth.json` detection with the index's
folder writer. Open snapshots `truth: files | store` into the folder descriptor;
the index inherits it. Missing metadata means files. Invalid/unreadable,
oversized or linked metadata is refused before cache writes can be authorized.
Store truth does not remove ordinary file-write capabilities: the temporary
code-store write restriction lives in the index writer. See [[Indexing]] and
[[ADR-0030 Where the truth lives]] for the marker and stopgap.

WP-56: read-only `Text{recovery-path}` admits only current ignore-aware regular files without symlink components. It validates 4096-byte/64-component paths and bounds each ancestor listing to 10,000 entries on a blocking worker; unsupported or excluded paths provide no recovery node. Ordinary `path` still resolves hidden files for existing settings/editor uses.
