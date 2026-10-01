---
title: "Problem Ranking"
description: The anticipated hard problems (R-01 … R-41), ranked by difficulty and risk, with where each stands after Milestone 17.
tags: [problems, planning]
---
The problems foreseen *before* building, ranked. Difficulty 1–5 (effort + unknowns); Risk = how much else breaks if it goes wrong. Problems actually *hit* while building are logged in the [[Problem Log]] as `P-nnn`; what is being worked on next is the [[Roadmap]]; the current state per area is [[Status]].

> [!note] Two id spaces
> Until 2026-10-01 these rows were numbered `P-01 … P-35`, which collided with the Problem Log's `P-032 … P-035` (P-32 "git" here, P-032 "release-build observability" there). The ranking now uses **`R-nn`** everywhere in the vault; the log keeps `P-nnn`.

Status, as of Milestone 17 (2026-09-29): ✅ done · ◐ partly done (what is missing is said) · ○ open.

| # | Problem | Diff | Risk | State |
|---|---|---|---|---|
| R-01 | Core graph model + `Source` trait + query IR | 3 | **very high** | ◐ `Node`/`Edge`/`Source`/`Query` exist and every source uses them; **not built**: `GraphView` (query + watermark), node/edge properties, `Source::subscribe` (`changes_since` long-poll instead), the lifting rules in `sources::lift` — [[Graph-Native Model]] |
| R-02 | Folder source (native) + explorer panel | 2 | low | ✅ watched since Milestone 16 |
| R-03 | JS interop protocol (`eval` + channel) on all webviews | 3 | medium | ✅ web, WebKitGTK, Android WebView (P-047 rules) |
| R-04 | Code editor with CodeMirror behind `CodeEditorBackend` | 3 | medium | ✅ splices since spec [[018]] |
| R-05 | Extension API + static registry; built-ins as extensions | 3 | **high** | ◐ `Extension` trait (panels, commands, settings, flow libraries), tiers, permissions. **Not built**: `moonkale.toml`, activation events, `when` clauses, contribution points beyond panels/commands — and `ui` still hard-codes the catalogue ([[Extension Catalogue]] critique) |
| R-06 | tree-sitter index + wiki-link extraction | 3 | medium | ◐ native tree-sitter (arborium), wiki-links; Julia/Python extractors exist but never run (issue #16) |
| R-07 | Graph view v1: wgpu 2D, WebGL2/WebGPU, CPU layout, pick/popup | 4 | **high** | ✅ |
| R-08 | SQLite + DuckDB sources + table editor | 3 | low | ✅ read-only |
| R-09 | Markdown editor: source mode, links, backlinks, local graph | 2 | low | ✅ |
| R-10 | Remote source via `api` (web/mobile parity) | 3 | medium | ✅ token auth (M7), TLS (M11) |
| R-11 | Milkdown WYSIWYG behind `RichTextBackend` | 3 | medium | ✅ |
| R-12 | Typst preview | 2 | low | ✅ no packages |
| R-13 | Terminal: PTY + xterm view + links | 2 | low | ✅ plus a Rust twin (M12) |
| R-14 | LSP client + local spawning | 3 | medium | ✅ full-document `didChange` still (issue #19) |
| R-15 | Graph DB sources: Ladybug, Falkor | 2 | low | ◐ LadybugDB (Linux only since M17, P-144); FalkorDB not started |
| R-16 | Cross-source edges + consistency | 4 | **high** | ○ needs projects ([[Projects and Sources]]) |
| R-17 | LLM gateway: providers, tool surface, policy, audit | 3 | medium | ✅ Claude Code, Anthropic, OpenAI-compatible, Ollama, mock |
| R-18 | Embeddings + hybrid search | 3 | medium | ✅ BM25 + brute-force cosine, in memory |
| R-19 | Postgres/Supabase, Turso sources | 2 | low | ◐ Turso embedded (M17, read-only); Postgres not started |
| R-20 | Remote LSP + remote terminal on `api` (security) | 4 | **high** | ◐ one token per server; the audit found gaps ([[Audit 2026-09-23]]); no accounts |
| R-21 | Stack-trace / AST → graph | 2 | low | ◐ traces; ASTs not |
| R-22 | GPU compute layouts; 100k+; desktop surface ([[P-001 Graph surface in desktop webview]]) | 5 | **very high** | ◐ Barnes–Hut on the CPU reaches 100k; surface plan A in use; GPU compute and the coarse tier ([[Case Selector]]) open |
| R-23 | wasmtime extension runtime + WIT world + permissions UI | 4 | **high** | ◐ JSON ABI v1 over core modules ([[ADR-0013 JSON ABI before components]]); no WIT, no UI contributions, no fuel limits (issue #4) |
| R-24 | Flow editor + Lux.jl codegen | 3 | medium | ◐ editor + codegen (M6); running the model with errors on blocks open |
| R-25 | Mobile: file access, collapsed shell, touch | 4 | medium | ◐ Android build, phone shell, gestures, touch drag; Storage Access Framework open (spec [[028]]) |
| R-26 | TypeDB, HelixDB sources | 3 | medium | ◐ HelixDB embedded (M17, read-only, P-145); TypeDB not started |
| R-27 | 3D graph | 3 | low | ✅ planes per kind, orbit camera |
| R-28 | Browser-side WASM extensions | 5 | **very high** | ✅ Worker + SharedArrayBuffer, JSON ABI |
| R-29 | Rust-native backends (terminal → code → rich text) | 5 | research | ◐ terminal (M12) and code (M14) as opt-in twins; rich text none |
| R-30 | Collaborative editing (CRDT over the op stream) | 5 | research | ○ |
| R-31 | Structured Typst / Excalidraw-in-flow | 4 | research | ○ |
| R-32 | Git integration: status/diff, commit/log, history as a graph | 3 | medium | ✅ through the `git` CLI (not `gix`); no push/pull |
| R-33 | Entity log: events, fold, snapshots, checkpoints ↔ commits | 4 | **high** | ✅ JSONL per folder, compaction (M9); issue #17 open |
| R-34 | Presence: awareness, hub, desktop + web | 3 | medium | ✅ rooms, cursor lines, desktop hub client (M9) |
| R-35 | Windows / macOS / iOS builds | 3 | medium | ◐ CI builds Windows and macOS packages; a friend tests macOS; no iOS |
| R-36 | **One store for internal state** (settings, layout, history, sessions, index) instead of a dozen files, ready to sync | 4 | **very high** | ○ candidates in Milestone 17; comparison pending — [[Internal State]], [[ADR-0014 One store for internal state]] |
| R-37 | **The core as a library**: `ui` without extension or driver dependencies, `Workspace` split into services, a server contribution point, reusable crates (graph renderer, core) | 4 | **high** | ○ planned — [[Milestone 18 - Library Refactor]] |
| R-38 | **Security findings of the external audit** (issues #1–#5, #7–#10, #18, #20) | 3 | **high** | ○ — [[Audit 2026-09-23]], [[Security]] |
| R-39 | **CI that runs the tests**: no workflow runs `cargo test`, clippy, fmt or an E2E suite today | 2 | **high** | ○ — [[Testing Strategy]] |
| R-40 | Projects: several sources saved as a project, selector, sync | 4 | medium | ○ desired behaviour in [[Projects and Sources]] |
| R-41 | Writes to databases (OLTP) — every database source is read-only | 3 | medium | ○ announced after Milestone 17 |

## Reading the table
- What is left of the original top risks: **R-01** (the model's missing half), **R-05** (the extension API's missing half), **R-16**, **R-20** and **R-22**. **R-36** and **R-37** are new and have the widest blast radius — every crate touches the state and the `Workspace`.
- R-37 and R-39 come before new features: they are what makes the rest cheap to change. R-38 comes before any server is exposed beyond a trusted network.
- Anything marked *research* has no committed date.
