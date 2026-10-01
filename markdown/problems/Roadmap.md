---
title: "Roadmap"
description: What comes next and in which order — the library refactor, the internal store, then the features waiting for them — and the seventeen milestones so far.
tags: [problems, planning]
---
Where things stand today is [[Status]]; the anticipated hard problems and their state are the [[Problem Ranking]] (`R-nn`). This page is the **order**.

## Next

| # | what | why now | depends on |
|---|---|---|---|
| 1 | **[[Milestone 18 - Library Refactor]]** phases 0–4: CI, pruning, layering, the `Workspace` as services, the catalogue out of the shell, server contributions | every later feature is cheaper and safer after it; CI first (R-39) | — |
| 2 | **State interface (stable for redb) → internal-store comparison** → [[ADR-0014 One store for internal state]] accepted → refactor phase 5 | announced by Daniel after Milestone 17; settings, history, sessions, projects and a persisted index all wait for it (R-36) | 1 (phase 3 makes the services it plugs into) |
| — | **Local fixes from the audit** (#2, #5, #10, #11, #14, #15, close #6) | small, independent, some are data loss | nothing; any time |
| 3 | **Writes to database sources** (R-41) | every source is read-only today; announced after Milestone 17 | 1 (openers, classifier on the source) |
| 4 | **Projects** (R-40, [[Projects and Sources]], spec [[003]]) | cross-source links (R-16), per-source colour and read-only, sync | 2 (projects live in the store) |
| 5 | **The Lenticulum path** ([[Julia and Lenticulum]]): flow editor with undirected ports and nested subgraphs → MTK library → a live graph source and the factor-graph viewer | the reason Moonkale exists | 1 (graph renderer as a library, `moonkale-julia` repo) |
| 6 | **Graph coarse tier** ([[Case Selector]]) and the plain-text tier | factor graphs and code graphs past 100k nodes | 1 |

Candidates, documented and not ordered yet: **spec [[030]]** (English/German/Chinese, themes — best after refactor phase 4, when each extension brings its own strings and settings), [[Unicode Input]] (spec [[004]]), [[Markdown Diagrams and Math]] (fence renderers), [[Annotations]], [[Notebook Editor]], [[Publishing Sources]] (reader mode), [[Collaboration]] (CRDT editing), accounts and roles for a shared server ([[Remote and Server Modes]]), the Julia depot as a read-only source, specs [[024]]/[[025]] (go to definition from the cursor, five-button mouse), spec [[028]] (phone vault export, Storage Access Framework), the remaining JS → Rust replacements ([[JavaScript Inventory]]), Postgres/Redis/TypeDB/FalkorDB sources.

## Milestones so far

| Milestone | You can… | Plan / log |
|---|---|---|
| **1 Walking skeleton** ✅ | open a folder, edit and save files in a dockable workbench, on desktop and web (server-side folder) | the model, the interop boundary, the extension API — **done 2026-09-17**, see [[Milestone 1 - Walking Skeleton]] / [[Milestone 1 - Implementation Log]] |
| **2 Graph appears** ✅ | see your folder + wiki-links + symbols as a graph; open a SQLite file and browse tables; edit markdown with backlinks | "graph-native" is real; wgpu works in webviews — **done 2026-09-18** (DuckDB deferred), see [[Milestone 2 - Implementation Log]] |
| **3 Databases & tools** ✅ | use a terminal, preview Typst, get LSP diagnostics/hover/definition, open a LadybugDB and draw Cypher results — on desktop and web (tools run on the server) | server-as-backend; second/third interop packages — **done 2026-09-18** (Postgres/Falkor/Milkdown deferred to 4), see [[Milestone 3 - Implementation Log]] |
| **4 Agents** ✅ | chat with an agent that queries your sources under policy; hybrid search; click a stack trace into a graph | LLM-as-user — **done 2026-09-18** (mock-verified; real providers wired but untested here), see [[Milestone 4 - Implementation Log]] |
| **5 Settings & writing** ✅ | remember layouts, folders and providers; configure the agent in the UI; WYSIWYG markdown; an agent that edits and runs under the gate; MCP for external agents | persistence + the writing half — **done 2026-09-18**, see [[Milestone 5 - Implementation Log]] |
| **6 Scale & extend** ✅ | 100k-node graphs; install third-party wasm extensions; build a Lux.jl model by drag-and-drop; use it on a phone | the two hardest bets — **done 2026-09-19** (Barnes–Hut instead of GPU compute; wasm ABI v1 instead of components; phone shell verified at 420 px on web, no Android build here), see [[Milestone 6 - Scale and Extend]] / [[Milestone 6 - Implementation Log]] |
| **7 Daily driver** ✅ | command palette and quick open; file operations; find/replace; LSP completion and rename; git status/diff/commit and history as a graph; a web server you can expose | using Moonkale on its own repo every day — **done 2026-09-19**, see [[Milestone 7 - Daily Driver]] / [[Milestone 7 - Implementation Log]] |
| **8 Research** ✅ | entity log and history panel; presence across machines; 3D graph; wasm extensions in the browser | long-term direction — **done 2026-09-19**, see [[Milestone 8 - Research]] / [[Milestone 8 - Implementation Log]] (Postgres/TypeDB, Android and the JS-free desktop wait for an environment) |
| **9 Second halves** ✅ | history you can act on (snapshots, restore); presence cursors and a desktop hub client; 3D node dragging; DuckDB and data-file tables; the Android build | **done 2026-09-19** — the release APK runs on a Galaxy S10e (phone shell, Explorer, editor with the soft keyboard, save + history, graph on WebGL2) after five Android-only fixes — [[Milestone 9 - Second Halves]] / [[Milestone 9 - Implementation Log]] |
| **10 Daily use** ✅ | use Moonkale every day; the small specifications are the backlog — specs 001–019 all done (formulas, wiki-links, highlighting, wrap, close folder, closeable panels, image viewer, activity bar + menus + source icons, Android gestures/name/icon, splices, front matter) | **done 2026-09-21** — [[Milestone 10 - Daily Use]] / [[README|specifications]] |
| **11 Remote** ✅ | open folders on another machine over SSH from the desktop app (Zed's model); the desktop as a client of any Moonkale server; a standalone `moonkale-server`; TLS, terminal switch and Origin checks for exposed servers | **done 2026-09-21** (the desktop dialog awaits Daniel's look) — [[Milestone 11 - Remote]] / [[Milestone 11 - Implementation Log]] |
| **12 Agents & native terminal** ✅ | Claude Code as a provider (subscription, no key); agent sessions on the server that finish without a client; Connect to Server on desktop and phone; the first JS-free editor (a Rust/Dioxus terminal) with a chooser; an Extensions activity | **done 2026-09-21** — [[Milestone 12 - Agents and a Native Terminal]] / [[Milestone 12 - Implementation Log]] |
| **13 Packaging** ✅ | packages friends can install (pacman, apt, nix, tarball), a release workflow with unsigned Windows/macOS bundles, extension settings with the extension | **done 2026-09-21** — [[Milestone 13 - Packaging]] / [[Milestone 13 - Implementation Log]] |
| **14 Rust code editor** ✅ | a second code editor on `dioxus-code-editor` (tree-sitter in Rust/wasm for every core language), per-document switching, the caret's word as a signal, the Rust terminal's rows | **done 2026-09-21** — [[Milestone 14 - Rust Code Editor]] / [[Milestone 14 - Implementation Log]] |
| **15 Agents, profiles and connections** ✅ | saved agents chosen per session, sessions side by side with history in the folder, saved SSH connections, Settings without the extension list, Claude Code login | **done 2026-09-22** — [[Milestone 15 - Agents, Profiles and Connections]] / [[Milestone 15 - Implementation Log]] |
| **16 Sources follow the disk** ✅ | the Sources panel follows changes on disk (↻ for unwatched sources), Local graph laid out on its own, no `<br />` from the rich editor, tabs dragged by touch | **done 2026-09-28** — [[Milestone 16 - Sources Follow the Disk]] / [[Milestone 16 - Implementation Log]] |
| **17 Embedded stores** ✅ | Turso, redb, RocksDB and embedded HelixDB as read-only sources; LadybugDB linked statically with localized symbols (Linux) | **done 2026-09-29** — [[Milestone 17 - Embedded Stores]] / [[Milestone 17 - Implementation Log]] |

## Principles for sequencing
1. **Risky things early, but not first.** The core model came first because it had to; the extension API waited for one real editor; the store waits for its comparison.
2. **Every milestone ends runnable on at least two platforms.** No "we'll do web later" — that is how single-platform assumptions leak in.
3. **Built-ins go through the extension API.** Retrofitting is how privileged paths appear — and where they appeared anyway (the catalogue in `ui`, driver calls in the Explorer), [[Milestone 18 - Library Refactor]] removes them.
4. **Log every problem** in the [[Problem Log]] (`P-nnn`, [[Problem Template]]); update [[Status]] at the end of every milestone.
