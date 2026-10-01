---
title: "Internal state — what Moonkale keeps, where, and the one store it should become"
description: Every piece of state Moonkale writes for itself today (per folder, per user, per platform, derived), why a dozen files is a problem, the four embedded stores pulled in by Milestone 17 as candidates, and what the comparison has to answer before the store is chosen.
tags: [architecture, state, storage, design]
---
Daniel, 2026-09-28: the embedded stores of [[Milestone 17 - Embedded Stores]] — Turso, redb, RocksDB, HelixDB — were pulled in mainly to **compare them as the store for Moonkale's own state**. Everything except secrets would live in one or two of them on the machine, and once the server works, sync between devices. **Databases as sources stay extensions** (LadybugDB included); that is a separate concern from this page. Decision record: [[ADR-0014 One store for internal state]] (proposed).

## What exists today (surveyed 2026-09-28, checked 2026-10-01)

| scope | what | where | format | written by |
|---|---|---|---|---|
| per folder | workspace settings, layout, open documents, extension grants | `<folder>/.moonkale/settings.json` | JSON (`SettingsFile`) | `Workspace::update_workspace_settings` |
| per folder | entity log (every change: who, when, what) | `<folder>/.moonkale/history.jsonl` | JSON lines, compacted into snapshots | `Workspace::record*`, `core::graph::history` |
| per folder | agent sessions (local) | `<folder>/.moonkale/agent-sessions/local/<id>.json` | JSON | `editors/agent` |
| per folder | agent sessions (server) | `<folder>/.moonkale/agent-sessions/<id>.jsonl` | JSON lines | `api/agent_sessions.rs` |
| per folder | saved chats | `<folder>/.moonkale/chats/*.md` | markdown pages | `editors/agent` |
| per folder | deleted files | `<folder>/.moonkale/trash/` | files | `project-fs` |
| per folder | KaTeX macros | `<folder>/.moonkale/katex.json` | JSON | the user |
| per folder | wasm extensions | `<folder>/.moonkale/extensions/*.wasm` | modules | the user |
| per user, desktop | user settings, recent folders, saved agents, saved SSH connections | `<config dir>/moonkale/settings.json` | JSON | `desktop/src/main.rs` |
| per user, web | user settings | the browser's `localStorage` | JSON | `web/src/main.rs` |
| per user, Android | user settings | `<files dir>/settings.json` | JSON | `mobile/src/main.rs` |
| per user | wasm extensions | `<config dir>/moonkale/extensions/` | modules | the user |
| per user | **secrets** | `<config dir>/moonkale/secrets.json` (mode 600) + environment | JSON | Settings → secret | 
| derived | the index: graph, BM25, embeddings | memory only, rebuilt on every open | — | `moonkale-index` |

Problems with this, each seen in practice:
- **Nothing syncs.** A second device starts from zero; the web client's settings are per browser.
- **App state lands in git.** This repository committed its own `.moonkale/history.jsonl` and `settings.json` until 2026-10-01 (now ignored); every session that opens it changes them (merge noise in the 2026-09-26 `session-pause` merge), and a cloned repository's settings are trusted more than they should be ([[Audit 2026-09-23]] #8, #20).
- **The entity log is a JSON-lines file** with an O(n²) merge and compaction that hides old renames (P-086, issue #17).
- **The index is rebuilt on every open.** On this repository that is seconds; on a large folder or with embeddings it is minutes, and embeddings cost money with a hosted model.
- **Three code paths for one thing** (user settings on desktop, web and Android), each with its own bugs.

## The store, as a requirement list
1. **Embedded, one file or directory per scope**, native on desktop, server and Android; the browser keeps going through the server (or OPFS later).
2. **Holds**: settings (user, project, folder scopes), layouts, open documents, saved agents and connections, agent sessions, the entity log, projects ([[Projects and Sources]]), annotations ([[Annotations]]), and a persisted index (graph, BM25 postings, embeddings with model + version).
3. **Never holds secrets.** They stay in the secret store (keychain later).
4. **Syncs** through the server hub ([[Collaboration]]): append-only history merges as sets of events; settings last-writer-wins per key with the loser kept; the index is never synced (derived).
5. **Keeps the folder clean**: nothing under `<folder>/.moonkale/` that changes on every open. What a folder deliberately shares with its collaborators (KaTeX macros, a folder's own wasm modules, shared annotations) stays a plain file there; everything else moves to the user's store, keyed by the folder's id.
6. **Content-addressed identity possible**: the Sophia and Unison use cases ([[Julia and Lenticulum]], the code-graph compiler idea) want nodes keyed by a hash with many names → one node; the schema must not assume a node is a path.

## The candidates (from Milestone 17)

| store | kind | in the tree because | for internal state |
|---|---|---|---|
| **Turso** (`turso` 0.7) | SQLite rewritten in Rust (beta), in-process | M17 source | SQL and a known file format; good for settings, projects, sessions, the log as a table; beta |
| **redb** (`redb` 4) | pure-Rust B-tree key/value, ACID, one file | M17 source | smallest dependency, no C; typed tables; you write the indices yourself |
| **RocksDB** (`rocksdb` 0.25) | LSM key/value (C++) | M17 source | fastest writes, column families; heavy C++ build; bindgen needs libclang on every CI platform |
| **HelixDB embedded** (git) | graph + vector over SlateDB | M17 source, spec [[002]] | graph and vectors in one store fits the index; git dependency on an unpublished crate, writes a manifest even when opened read-only (P-145) |
| SQLite (`rusqlite`, bundled) | the reference | the SQLite source | what every other option is compared against |

A likely shape, to be confirmed by the comparison: **one relational/key-value store for the state** (settings, projects, sessions, the log) and, separately, **a persisted index** that may use a graph/vector store. Two stores at most.

## What the comparison has to measure
- Cold-open and write latency for the log (10⁵–10⁶ events) and for settings writes (every layout change).
- Build cost (clean build seconds, binary size) on Linux, Android, Windows and macOS — RocksDB and Helix bring C++ and a git tree.
- Android: does it build for `aarch64-linux-android`, and how big is the APK afterwards?
- Crash safety: kill the process mid-write, reopen.
- Concurrency: two windows of one user (the session bus today), the server writing while a client reads.
- Sync: how a merge of two devices' logs is expressed (rows by event id; a key range scan).

## Migration
Read the old files once, write the store, leave the files in place for one release, then stop reading them. `.moonkale/history.jsonl` and `.moonkale/settings.json` in this repository get removed from git and ignored when the store lands.

Related: [[Version Management]] (the log's model), [[ADR-0012 Two histories]], [[Milestone 18 - Library Refactor]] (phase 5), [[Database Backends]].
