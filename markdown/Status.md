---
title: "Status"
description: What Moonkale can do today, area by area — built, partly built, designed only — checked against the code on 2026-10-01 (after Milestone 17), with the open problems that matter most. The one place where status is kept.
tags: [status, moc]
---
**This page is the single place for "where are we".** The landing page, [[Home]], the README and the [[Roadmap]] link here instead of repeating it. Update it at the end of every milestone. Checked against the code on **2026-10-01** (`refactor` branch = `master` + Milestone 17).

Legend: ✅ built and tested · ◐ partly built (the missing part is named) · 📝 designed only, no code · ⚠ built but broken

## Numbers
40 crates · 46 900 lines of Rust · ~1 000 lines of TypeScript in four view bundles · 146 Rust tests · 47 browser suites (Playwright) · CI since 2026-10-01: fmt, layering rules, tests, clippy, wasm check, six browser suites (R-39) · releases `YYMMDD-proto` for Arch, Debian, Nix, tarball, Windows, macOS, Android ([[Install]]).

## By area

| area | state | notes |
|---|---|---|
| **Workbench** (docking, rail, menus, palette, quick open, keybindings, phone shell) | ✅ | [[ADR-0010 dioxus-workbench for layout]], spec [[009]], [[011]] |
| **Folders** (open several, watch disk, file ops, trash, find/replace) | ✅ | Milestone 7, 16; races in issue #15; symlinks escape the root (#5) |
| **Remote folders over SSH**, desktop as a server's client | ✅ | Milestone 11; hardening #18 |
| **Projects** (saved sets of sources, selector, sync) | 📝 | [[Projects and Sources]], spec [[003]] |
| **Code editor** — CodeMirror (LSP, highlighting, wiki-links, presence) | ✅ | splices (spec [[018]]); full-text LSP sync (#19) |
| **Code editor** — Rust (`dioxus-code-editor`) | ◐ | view + edit + highlight; no LSP or decorations — [[Code Editor Implementations]] |
| **Markdown** — rich (Milkdown), source, wiki-links, KaTeX, front matter, typography | ✅ | specs 012, 013, 019, 021, 026 |
| **Typst preview** | ✅ | no packages |
| **Diagram fences** (tabs, Typst math, Mermaid, TikZ) | 📝 | [[Markdown Diagrams and Math]] |
| **Graph view** (wgpu 2D/3D, Barnes–Hut, Local mode, several folders) | ✅ | 100k nodes on the CPU; coarse tier 📝 ([[Case Selector]]); stale drag-index panic (#12) |
| **Flow editor** + Lux.jl library | ◐ | editing and codegen; running the model is not built; destroys an unparseable file (#11) |
| **Table editor** | ✅ | read-only |
| **Image viewer** | ✅ | spec [[008]] |
| **Terminal** — xterm and Rust | ✅ | traces become graphs |
| **Notebook editor** | 📝 | [[Notebook Editor]] |
| **Agent** (sessions, saved agents, policy gate, MCP, server sessions, Claude Code) | ✅ | [[Agent Sessions and Profiles]]; wedges (#13) |
| **Search** (BM25 + embeddings) | ✅ | in memory, rebuilt on open |
| **Index**: wiki-links, Rust symbols | ✅ | |
| **Index**: Julia, Python symbols | ✅ | fixed 2026-10-02 on `refactor` (issue #16 — the extractors never ran before) |
| **Index**: calls/references, other languages | 📝 | [[Core Languages]] |
| **Git** (changes, diff, stage, commit, history graph) | ✅ | the `git` CLI; no push/pull |
| **History** (entity log, restore, compaction) | ✅ | one row per event in the folder host's state store (Milestone 18 phase 5.10; was JSONL); #17, P-086 |
| **Presence** (who's here, cursor lines, desktop hub client) | ✅ | |
| **Shared editing** (CRDT) | 📝 | [[Collaboration]] |
| **Annotations** | 📝 | [[Annotations]] |
| **Unicode input** (`\int` → ∫) | 📝 | [[Unicode Input]], spec [[004]] |
| **Server** (`moonkale-server`: token, TLS, jail, audit, terminal switch) | ◐ | single token, no accounts; eleven open security findings ([[Audit 2026-09-23]]) |
| **Reader mode** (published, queryable sources) | 📝 | [[Publishing Sources]] |
| **Extensions — static** (Rust crates, tiers, permissions, settings per extension) | ✅ | the catalogue is hard-coded in `ui` ([[Extension Catalogue]]) |
| **Extensions — wasm** (JSON ABI v1, commands/tools; wasmtime + browser Worker) | ◐ | no UI contributions, no manifest file, no budgets — [[ADR-0013 JSON ABI before components]] |
| **Extension manifest, activation events, `when` clauses, contribution points beyond panels/commands** | 📝 | [[Contribution Points]], [[Manifest Reference]], [[Host API Reference]] describe the target |

## Sources

| source | state | platforms |
|---|---|---|
| Folder | ✅ watched | all (Android: private storage only, spec [[028]]) |
| SQLite, DuckDB (+ CSV/TSV/Parquet folders) | ✅ read-only | desktop, server |
| Turso, redb, RocksDB, HelixDB (embedded) | ✅ read-only (Milestone 17) | desktop, server |
| LadybugDB | ✅ read-only | **Linux only** (P-144) |
| Postgres/Supabase, Redis/Dragonfly, TypeDB, FalkorDB, HelixDB (HTTP) | 📝 | stub files only |
| Git-host repositories, REST/GraphQL APIs, Julia depot | 📝 | [[Projects and Sources]], [[Julia and Lenticulum]] |
| Writes to any database | 📝 | announced after Milestone 17 (R-41) |

## Platforms

| platform | state |
|---|---|
| Linux desktop (Arch, NixOS), web, server | ✅ tested here |
| Android (Galaxy S10e) | ✅ release APK, signed; no drivers, no PTY, no wasm runtime on the phone |
| macOS | ◐ CI builds; a friend tests source builds (spec [[027]] unverified) |
| Windows | ◐ CI builds; untested |
| iOS | 📝 |

## Most important open problems
1. **CI is new** (R-39, phase 0 of [[Milestone 18 - Library Refactor]]) — only six of the 47 browser suites run there.
2. **Security findings** — [[Audit 2026-09-23]], [[Security]]. #1, #4, #8 fixed in their structural part on `refactor` (Milestone 18 phase 4.5); the others are open. Don't expose a server beyond a trusted network.
3. **The core is a library** on `refactor`: `moonkale-core`, `moonkale-ext-api` (documented, `CHANGELOG.md`) and `moonkale-graph-render` (a Rust API, no Moonkale deps) tagged `lib-v1`; the shell depends on no extension; no layering exceptions. `Workspace` is still one large facade — R-37, [[Milestone 18 - Library Refactor]].
4. **Internal state**: SQLite chosen ([[ADR-0014 One store for internal state]]); layouts, user settings, the entity log and agent sessions are in the store on `refactor` (phase 5); nothing syncs yet — R-36, [[Internal State]].
5. Release-build observability (P-032): no log file or panic dialog in installed desktop builds ([[Debugging and Logging]]).

## Next
[[Milestone 18 - Library Refactor]] is done on the `refactor` branch ([[Milestone 18 - Implementation Log]]): the library crates are tagged `lib-v1`, Lux.jl lives in `MathStruct/moonkale-julia` (feature `julia`). Next: merge `refactor`, then spec [[030]] (languages and themes). The order beyond that is in the [[Roadmap]].
