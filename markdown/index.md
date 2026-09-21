---
title: Moonkale
description: A graph-native code and knowledge editor — folders and databases as one graph, extension-driven, built in Rust with Dioxus.
---
![[assets/MoonkaleBanner.png|The Moonkale banner: a wizard tending a kale plant under a full moon]]

| Desktop | Phone |
|---|---|
| ![[assets/UIDesktop.png\|Moonkale on the desktop: activity bar, Explorer, the vault as a graph, a note in the rich editor]] | ![[assets/UIAndroid.jpg\|Moonkale on a phone: a rich note above a Rust file, the bottom bar]] |

*The look on 2026-09-20 — the vault's own graph next to a note in the rich editor; on the Galaxy S10e a note over a Rust file.*

**Moonkale** is a code and knowledge editor built on one idea: *everything you open becomes a graph*. A folder of source files, a Postgres schema, a TypeDB database, a Redis keyspace and an Obsidian-style wiki all become nodes and edges in the same model — and every editor (code, markdown/Typst, tables, a GPU graph view, a no-code flow canvas, a terminal) is a view on that graph. Language servers, indexers and LLM agents work on the same graph through the same doors.

It is written in Rust with [Dioxus](https://dioxuslabs.com), targets desktop, web and mobile from one codebase, and is extension-driven: the built-in editors are themselves extensions with no privileged access.

> [!info] Status — Milestone 11 done: remote folders over SSH (2026-09-21)
> **Remote.** `File → Open Remote Folder…`: the system `ssh` in a terminal tab (your keys, agent, passwords and host-key prompts, untouched by Moonkale), Moonkale's own server copied to the host once per version and started on its loopback with a per-session token over stdin, the port forwarded — the folder, index, LSP, git and terminal run *there*, the editor, the graph and the LLM keys stay *here*; closing the folder ends everything. A standalone `moonkale-server` with built-in TLS, a terminal switch and Origin checks for the servers you expose. See [[Milestone 11 - Implementation Log]].
> **Second halves.** History you can act on (compaction into snapshots, **Restore** any earlier text as an unsaved edit with provenance); presence with **cursor lines** in the editor gutter and a **desktop hub client**; node dragging in 3D; **DuckDB** — `.duckdb` files and folders of CSV/TSV/Parquet as queryable tables. And the first **Android** build: the release APK runs on a Galaxy S10e — phone shell, editor with the soft keyboard, save + history, graph on WebGL2 (five Android-only fixes, P-087–P-091). See [[Milestone 9 - Implementation Log]] · [[Milestone 10 - Daily Use]] · [[Milestone 11 - Remote]]. Before that, Milestone 8 —
> **Research.** An **entity log** of every change in a workspace (who, when, what — user, agent or git checkpoint) with a **History** panel that shows any file as it was after any event; **presence** — who else is in the folder and what they look at; a **3D graph** with one plane per node kind and an orbiting camera; and **wasm extensions running in the browser** (Worker + SharedArrayBuffer, host calls answered by the client). See [[Milestone 8 - Implementation Log]]. Before that, Milestone 7 —
> **Daily driver.** A **command palette** and **quick open** over a command registry with rebindable keys; **file operations** in the Explorer (new, rename, move, delete to trash) that open documents and the index follow; **find & replace** in a file and across the workspace; the **second half of LSP** (completion, rename, code actions, references); **git** as a Changes panel with diffs, staging, commits and the **history drawn as a graph**; and an **access token** so the web server can be exposed. See [[Milestone 7 - Implementation Log]]. Before that, Milestone 6 —
> **Scale & extend.** Extensions are a managed catalog (Settings → Extensions: toggles and permissions; Flow editor and Lux.jl ship **off**); a **flow editor** on a node canvas with typed ports and block libraries contributed by extensions; a **Lux.jl library** that generates `model.jl`; **wasm extensions** (core modules, JSON ABI, wasmtime, permissions at the host boundary) whose commands become agent tools; **Barnes–Hut** layout (100k nodes at ~170 ms/step); a **phone-sized shell** below 700 px. See [[Milestone 6 - Implementation Log]]. Before that, Milestone 5 —
> **Settings & writing.** Moonkale now **remembers**: layout, open documents and recent folders per workspace, provider and policy choices in two scopes (machine and folder) with a **Settings panel** (`Ctrl+,`); markdown has a **Rich (WYSIWYG) mode**; the **agent can edit files and run commands** through diff/command cards under the policy gate; an **MCP endpoint** lets Claude Code and other external agents query the open workspace. Before that, Milestone 4 —
> **Agents.** An in-app **agent** (Anthropic / OpenAI-compatible / Ollama / mock) that lists sources, browses the graph, reads files, runs read-only SQL/Cypher and searches — every call through a **policy gate** with approval prompts and an audit log; transcripts saved as indexed pages. **Hybrid search** (BM25 + optional embeddings, Ctrl+Shift+F) and **stack traces drawn as graphs** from the terminal. On top of Milestone 3's terminal, Typst preview, LSP and LadybugDB; Milestone 2's index, wgpu graph view, backlinks and SQLite; Milestone 1's dockable workbench. See [[Milestone 4 - Implementation Log]], then [[Roadmap]] for Phase 9.

## Start here

- [[Overview]] — the system in one diagram
- [[Project Structure]] — the crate layout and why each crate exists
- [[Graph-Native Model]] — the one big bet
- [[Writing an Extension]] — because the project is extension-driven
- [[Problem Ranking]] → [[Roadmap]] — what is hard, and in which order

## Sections

| | |
|---|---|
| [[Home\|Vault home]] | full map of contents |
| Architecture | [[Overview]] · [[Platform Matrix]] · [[Data Sources]] · [[Projects and Sources]] · [[Julia and Lenticulum]] · [[Publishing Sources]] · [[Remote and Server Modes]] · [[Annotations]] · [[Indexing]] · [[Extension System]] · [[JS Interop Boundary]] · [[JavaScript Inventory]] · [[LLM and RAG]] · [[LSP and Terminal]] · [[Debugging and Logging]] · [[Version Management]] · [[Collaboration]] |
| Editors | [[Code Editor]] · [[Markdown and Typst Editor]] · [[Table Editor]] · [[Graph View]] · [[Flow Editor]] · [[Terminal]] · [[Unicode Input]] · [[Case Selector]] · [[Markdown Diagrams and Math]] · [[Notebook Editor]] |
| Extensions | [[Extension Catalogue]] · [[Writing an Extension]] · [[Contribution Points]] · [[Manifest Reference]] · [[Host API Reference]] · [[Claude Code Extension]] |
| Decisions | [[ADR-0001 Dioxus instead of Lumino and Tauri]] … [[ADR-0011 Desktop graph surface strategy]] |
| Platform | [[Linux Desktop Setup]] · [[Core Languages]] |
| Packaging | [[Packaging Overview]] · [[Arch Linux]] · [[NixOS]] · [[Android]] · [[Android Extensions and Bundling]] |
| Testing | [[Testing Strategy]] · [[How to Write Tests]] |
| Research | [[Database Backends]] · [[Graph Rendering Options]] · [[Rust-native Editor Candidates]] · [[WASM Extension Runtimes]] · [[Versioning Prior Art]] |
| Milestones | [[Milestone 1 - Walking Skeleton]] · [[Milestone 1 - Implementation Log]] · [[Milestone 2 - Graph Appears]] · [[Milestone 2 - Implementation Log]] · [[Milestone 3 - Databases and Tools]] · [[Milestone 3 - Implementation Log]] · [[Milestone 4 - Agents]] · [[Milestone 4 - Implementation Log]] · [[Milestone 5 - Settings and Writing]] · [[Milestone 5 - Implementation Log]] · [[Milestone 6 - Scale and Extend]] · [[Milestone 6 - Implementation Log]] · [[Milestone 7 - Daily Driver]] · [[Milestone 7 - Implementation Log]] · [[Milestone 8 - Research]] · [[Milestone 8 - Implementation Log]] · [[Milestone 9 - Second Halves]] · [[Milestone 9 - Implementation Log]] |
| Contributing | [[Development]] — building, running, and how this site is published |

Source: [github.com/MathStruct/Moonkale](https://github.com/MathStruct/Moonkale) · Part of [MathStruct](https://mathstruct.github.io/).
