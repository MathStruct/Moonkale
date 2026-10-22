---
title: "Status"
description: What Moonkale can do today, area by area — built, partly built, designed only — checked against the code on 2026-10-01 (after Milestone 17), with the open problems that matter most. The one place where status is kept.
tags: [status, moc]
---
**This page is the single place for "where are we".** The landing page, [[Home]], the README and the [[Roadmap]] link here instead of repeating it. Update it at the end of every milestone. Checked against the code on **2026-10-01** (`refactor` branch = `master` + Milestone 17).

Legend: ✅ built and tested · ◐ partly built (the missing part is named) · 📝 designed only, no code · ⚠ built but broken

## Numbers
40 crates · 46 900 lines of Rust · ~1 000 lines of TypeScript in four view bundles · 146 Rust tests · 47 browser suites (Playwright) · CI since 2026-10-01: fmt, layering rules, tests, clippy, wasm check, six browser suites (R-39) · releases `YYMMDD-proto` for Arch, Debian, Nix, tarball, Windows, macOS, Android ([[Install]]).

## Security CI (WP-47, checked 2026-10-17)

Blocking all-feature Rust licence/source/advisory checks, cargo-audit, six JS
lockfile audits and offline SHA-256 checks for 63 vendored files are implemented.
Existing Rust/npm findings are dated exceptions, awaiting Daniel's upgrade
choices; they are not fixed. Two separate non-blocking nightly parser fuzz jobs
run for 60 seconds each. Local runs completed 2,140,753 math and 8,218,925 flow
inputs without crashes; 18 parser tests and nine gate regression tests pass.
The first CI math crash (P-212) now has source-preserving production guards and
four corpus regressions; 216 code-view and five agent unit tests pass (code-view
uses the supported `DOCS_RS=1` registry-write bypass); browser verification remains due. The math normaliser remains planned; the target covers today's math range parser.
See [[Security]] and [[How to Write Tests]] for scope and commands. The page's
older whole-product inventory date above is unchanged; this paragraph checks only
WP-47, not every area below.

## Portable CI (WP-26, checked 2026-10-17)

Windows and Apple-silicon macOS now have an advisory `portable` matrix for core,
folder, index, extension API, LLM, Unison and remote library tests, followed by the
default extension distribution check. Native runs are pending GitHub validation;
UI, installers and Intel macOS are outside this CI job. See [[How to Write Tests]].

## By area

| area | state | notes |
|---|---|---|
| **Workbench** (docking, rail, menus, palette, quick open, keybindings, phone shell) | ✅ | [[ADR-0010 dioxus-workbench for layout]], spec [[009]], [[011]] |
| **Folders** (open several, watch disk, file ops, trash, find/replace) | ✅ | Milestone 7, 16; WP-23 (2026-10-17): metadata-only listings with separate fetched content versions, locked/rechecked saves, consistent directory versions, bounded directional neighbourhoods and native no-replace moves; final external-writer race remains #15; symlink swaps remain #5 |
| **Remote folders over SSH**, desktop as a server's client | ✅ | Milestone 11; hardening #18 |
| **Unison codebase source** | ✅ read-only, default-off | WP-18 (2026-10-17): local UCM 1.5 HTTP snapshot, store truth, content-addressed definitions/names/dependencies, hermetic schema tests and native graph-renderer payload test; `unison` feature, `*.unison` root opener; bounded snapshots, reopen for updates; see `packages/sources/unison/sources-unison.md` |
| **Sophia code store source** | ◐ default-off | WP-68 (2026-10-20): `*.sophia` stores through Sophia's `sophia-interface` (git rev): store truth, names/definitions/namespaces, text edits that move names, parse diagnostics, polling change feed, rendering profile as a `sophia:Settings` node, read-only `sql` and Sophia's `query` (table panel switch, views as tables, 2026-10-21); SPEC-004 §9 tests through `Source`. Missing: annotation edits, typings (Sophia M4); see `packages/sources/sophia/sources-sophia.md` |
| **Projects** (saved sets of sources, selector, sync) | 📝 | [[Projects and Sources]], spec [[003]] |
| **Code editor** — CodeMirror (LSP, highlighting, wiki-links, presence) | ✅ | splices (spec [[018]]); full-text LSP sync (#19) |
| **Code editor** — Rust (editor-core + Dioxus, opt-in) | ◐ | WP-69 (2026-10-17) compact icon toolbar (Save/Reload/Wrap/⋯), indentation settings in its gear popover, inline cursor word hidden on phones; preview typing regression fixed (P-211: 83 ms Preview / 73 ms Source with code fixes alone in the local development browser check); revisioned edits/history, highlighting, live browser/native WebKit LSP integration, browser/native AT-SPI accessibility semantics, tested backend migration, search/folds, Markdown preview and default-off Rust/Python/Julia documentation previews; latest strict release web frames 21/21 after the focus-border fix (prior preview-scrollbar failure retained); Rust highlight CPU time reduced, desktop software-GL timing still fails with quantized clocks; default prepared but unflipped; 2026-10-22: native FPS idle/input accounting corrected (P-225), session-specific undo under investigation (P-226); real-desktop frames and Android acceptance remain open — [[Rust Code Editor Implementation]], [[Rust Code Editor Roadmap]] |
| **Markdown** — rich (Milkdown), source, wiki-links, KaTeX, front matter, typography | ✅ | specs 012, 013, 019, 021, 026 |
| **Typst preview** | ✅ | no packages |
| **Diagram fences** (tabs, Typst math, Mermaid, TikZ) | 📝 | [[Markdown Diagrams and Math]] |
| **Graph view** (wgpu 2D/3D, Barnes–Hut, Local mode, several folders) | ✅ | 100k nodes on the CPU; coarse tier 📝 ([[Case Selector]]) |
| **Flow editor** + Lux.jl library | ◐ | editing and codegen; running the model is not built |
| **Table editor** | ✅ | read-only |
| **Image viewer** | ✅ | spec [[008]] |
| **Terminal** — xterm and Rust | ✅ | traces become graphs |
| **Notebook editor** | 📝 | [[Notebook Editor]] |
| **Agent** (sessions, saved agents, policy gate, MCP, server sessions, Claude Code) | ✅ | [[Agent Sessions and Profiles]]; wedges (#13) |
| **Search** (BM25 + embeddings) | ✅ | in memory, rebuilt on open |
| **Index**: wiki-links, Rust definitions | ✅ | WP-23: failed text reads carry a content-free node diagnostic, cleared on recovery; WP-65: shared L2 kinds, stable location ids, ranged occurrences, names and provenance; index rows rechecked 2026-10-15 — [[Code Graph Schema]] |
| **Index**: Julia, Python definitions | ✅ | WP-65: same shared schema; Julia overload ids use signatures rather than order |
| **Index**: Rust/Julia/Python calls, references and imports | ◐ | WP-22: conservative same-file syntax resolution, explicit unresolved targets; compiler-backed dispatch and cross-file resolution remain open — [[Code Graph Schema]] |
| **Index**: content identities (Rust/Julia/Python) | ◐ | WP-66: default-off explicit requests, versioned syntactic normalization, canonical recursion components, bounded caches and freshness guards; conservative syntax subset and same-file resolution, not semantic equivalence. Normal form and hashing are the crate `moonkale-content-id`, shared with Sophia, with golden fixtures; normalizer v2 (2026-10-20) — [[Code Graph Schema]] |
| **Code graph**: Rust lossless syntax and patch planning | ◐ | WP-67 slices 1–5 (2026-10-16): exact Rust plans and a revision-aware adapter, plus default-off structural-edits graph controls for declaration names and same-file moves on desktop/web; previews, grouped undo and explicit conflicts built; complete same-folder rename uses rust-analyzer with coordinated keyboard/menu/review history and explicit per-file saves; cross-file moves remain open — [[Rust Lossless Syntax and Patches]] |
| **Index**: other languages | 📝 | [[Core Languages]] |
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
1. **CI breadth** (R-39, WP-26) — default browser smoke suites plus the opt-in structural matrix; startup has a shared 60-minute budget, at most two inactivity-only attempts and retained diagnostics ([[P-209 CI startup stalls must retain evidence]]). Several suites remain local-only; three consecutive green runs are still the WP-26 acceptance target.
2. **Security findings** — [[Audit 2026-09-23]], [[Security]]. #1, #4, #8 fixed in their structural part on `refactor` (Milestone 18 phase 4.5); the others are open. Don't expose a server beyond a trusted network.
3. **The core is a library** on `refactor`: `moonkale-core`, `moonkale-ext-api` (documented, `CHANGELOG.md`) and `moonkale-graph-render` (a Rust API, no Moonkale deps) tagged `lib-v1`; the shell depends on no extension; no layering exceptions. `Workspace` is still one large facade — R-37, [[Milestone 18 - Library Refactor]].
4. **Internal state**: SQLite chosen ([[ADR-0014 One store for internal state]]); layouts, user settings, the entity log and agent sessions are in the store (phase 5); nothing syncs yet — R-36, [[Internal State]].
5. Release-build observability (P-032): no log file or panic dialog in installed desktop builds ([[Debugging and Logging]]).

## Next
[[Milestone 18 - Library Refactor]] is merged ([[Milestone 18 - Implementation Log]]; library crates tagged `lib-v1`, Lux.jl in `MathStruct/moonkale-julia`). Spec [[030]] — English/German/Chinese and themes ([[Languages and Themes]]) — is done on the `spec-030` branch; German and Chinese await a native review. The order beyond that is in the [[Roadmap]].
