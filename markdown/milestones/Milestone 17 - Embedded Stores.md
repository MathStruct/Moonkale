---
title: "Milestone 17 — Embedded stores: the plan"
description: redb, RocksDB, embedded HelixDB and embedded Turso as sources — opened from the Sources tree like SQLite and LadybugDB, browsable, queryable, drawn in the graph; read-only first, writes (the OLTP part) in the next steps. Record in Milestone 17 - Implementation Log.
tags: [milestone, planning, sources, databases]
---
From Daniel (2026-09-28): "This is a prototype, so a smaller core is not a concern. Pull in redb, RocksDB, the embedded HelixDB and the embedded TursoDB. I want some OLTP. More prompts on this later." Record: [[Milestone 17 - Implementation Log]]. Background: [[Data Sources]], [[Database Backends]], [[002]] (Helix).

## What each one is
| store | crate | kind | on disk | notes |
|---|---|---|---|---|
| **Turso** | `turso` 0.7 (crates.io, beta) | SQL (SQLite's dialect and file format, an in-process Rust rewrite with async I/O and MVCC) | one file | opens SQLite files too; `.db` stays with SQLite, so Turso opens `*.turso` |
| **redb** | `redb` 4 | typed key/value tables, ACID, copy-on-write B-trees, pure Rust | one file `*.redb` | tables have Rust types; only byte/str/integer typed ones can be read generically |
| **RocksDB** | `rocksdb` 0.25 (C++ via cc) | byte key/value LSM, column families | a directory | recognised as `*.rocksdb` folders; opened read-only (a secondary reader, no lock fight with a writer) |
| **HelixDB** | `helix-db` (git, pinned, `embedded`) | graph + vector (SlateDB underneath) | a directory `*.helix` | not on crates.io ([[002]]); heavy dependency tree |

## Scope (this step)
1. **Sources** behind features, native only (desktop app and server; never the browser or the phone app): `sources-sql` `turso`, `sources-kv` `redb` + `rocksdb`, `sources-graph` `helix`.
2. **Recognised in the Sources tree** by name (`*.turso`, `*.redb`, `*.rocksdb/`, `*.helix/`) and opened with a click, like SQLite/DuckDB/LadybugDB, on the desktop and through the server.
3. **Browsing**: database → tables (Turso: tables/views → columns; redb: tables with their key/value types; RocksDB: column families; Helix: node labels and edge labels).
4. **Queries in the table editor**: Turso speaks SQL (read statements, same gate as SQLite). redb and RocksDB get a small **`kv` dialect** — `scan <table> [prefix <p>] [limit <n>]`, `get <table> <key>` — keys and values shown as text when UTF-8, else hex. Helix gets `helixql` ([[002]]'s dialect slot) for a label listing to start with.
5. **Graph**: Turso's schema like SQLite; Helix's nodes and edges as a real graph (like LadybugDB's Data mode); key/value stores as database → tables.
6. Tests per source against a small store created in the test; the build cost measured (clean build before/after).

## Not in this step
- **Writes.** Everything opens read-only first, like SQLite today. The OLTP part — insert/update/delete from the table editor, transactions, the agent's write tools under the policy gate — is the next step (Daniel's follow-up prompts).
- Watching (`changes_since`) for these stores — they get the ↻ button of [[Milestone 16 - Sources Follow the Disk|Milestone 16]].

## Decisions
- **Turso by extension, not by sniffing.** Its files *are* SQLite files; choosing the engine per file needs a UI choice ("open with…"), which can come with writes. `*.turso` is unambiguous now.
- **RocksDB as a secondary instance** (`open_as_secondary`), so an application writing the same database keeps its lock; `try_catch_up_with_primary` before each read.
- **Helix pinned to one commit**, bumped by hand; its build cost is recorded.
