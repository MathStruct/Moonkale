---
title: "Moonkale — Vault Home"
tags: [moc]
---
![[assets/UIDesktop.png|Moonkale on the desktop, 2026-09-20: activity bar, Explorer, the vault as a graph, a note in the rich editor]]

Moonkale is a **graph-native code and knowledge editor**: it opens folders *and* database connections, shows everything as one graph, and is driven by extensions. This vault is the design record. Every choice has a note; every problem hit during implementation gets logged in [[Problem Log]].

> [!info] Start here
> 1. [[Overview]] — what the system is, in one diagram.
> 2. [[Project Structure]] — the crate layout and *why* each crate exists.
> 3. [[Problem Ranking]] and [[Roadmap]] — what is hard, and in which order to build.
> 4. [[Writing an Extension]] — the guide, because this project is extension-driven.

> [!tip] Vault
> The Obsidian vault root is the **repo root** (`Moonkale/.obsidian`); these notes live in `markdown/`. Installed plugins: Excalidraw, inline-tikz, wypst (Typst), tabs, document-comments. Diagrams here use Mermaid (core); Excalidraw/TikZ are available for richer ones.

## Architecture
- [[Overview]]
- [[Why Not a Plugin or Electron]] — why not a VS Code/Obsidian plugin, why Dioxus rather than Electron, and what Moonkale weighs (measured: ~80 MB private for the Rust side with a repo indexed, ~240 MB for the webview)
- [[Project Structure]] — rationale for `packages/`
- [[Platform Matrix]] — desktop / web / mobile: what runs where
- [[Graph-Native Model]] — nodes, edges, sources, views
- [[Data Sources]] — SQL, graph, KV, folders
- [[Publishing Sources]] — reader mode of the web server (queryable SQL/graph/search for visitors), not a static site; plots later (decision)
- [[Julia and Lenticulum]] — the goal behind Moonkale: a factor-graph editor where a diagram stays a diagram; Julia outside the core
- [[Extension Catalogue]] — every extension, tiers, platform fit, core vs separate repo, critique
- [[Claude Code Extension]] — Moonkale as Claude Code's IDE + Claude Code as an agent, no API key (plan)
- [[Markdown Diagrams and Math]] — Typst math (Rust), TikZ (desktop extension), Mermaid, tabs via one fence-renderer mechanism (design)
- [[Remote and Server Modes]] — SSH remote folders (Zed), a Moonkale server (code-server), a shared server; the security of each
- [[Notebook Editor]] — `.ipynb` viewer/editor with no new deps, kernels behind a feature (plan)
- [[Annotations]] — comments anchored to files/rows/nodes, GitHub issues as the same thing (plan)
- [[Case Selector]] — backend by size: rich/plain/viewer text, exact/approximate/coarse graph (design)
- [[Unicode Input]] — `\int` → ∫ everywhere (design)
- [[Core Languages]] — the languages Moonkale ships core extensions for, and where each stands
- [[Projects and Sources]] — several sources per saved project, selector, sync (desired behaviour)
- [[Indexing]] — tree-sitter, links, embeddings
- [[Extension System]]
- [[JS Interop Boundary]] — CodeMirror / Milkdown / xterm, kept replaceable
- [[JavaScript Inventory]] — where JS still is, distance to zero, node/deno/bun
- [[LLM and RAG]]
- [[LSP and Terminal]]
- [[Debugging and Logging]] — where output goes on each platform, and in compiled builds
- [[Version Management]] — git for files, an append-only UUID+timestamp entity log for the graph
- [[Collaboration]] — session bus (multi-window today) → presence/cursors → server hub → shared editing

## Platform
- [[Platform Matrix]] · [[Linux Desktop Setup]] — WebKitGTK, NVIDIA, WebGPU: what to change and why

## Packaging
- [[Packaging Overview]] — what `dx build` produces and where the binary looks for assets
- [[Arch Linux]] (PKGBUILD) · [[NixOS]] (flake) · [[Android]] (APK/AAB) · [[Android Extensions and Bundling]] (what can be added after install, app size, Play/F-Droid)

## Testing
- [[Testing Strategy]] · [[How to Write Tests]]

## Editors ("windows")
- [[Code Editor]] · [[Markdown and Typst Editor]] · [[Table Editor]] · [[Graph View]] · [[Flow Editor]] · [[Terminal]]

## Extensions
- [[Writing an Extension]] · [[Contribution Points]] · [[Manifest Reference]] · [[Host API Reference]] · [[Publishing and Platforms]]
- Examples: [[Example - Hello Panel]] · [[Example - Data Source]] · [[Example - Language]]

## Decisions (ADRs)
- [[ADR-0001 Dioxus instead of Lumino and Tauri]]
- [[ADR-0002 Rust first, TypeScript behind traits]]
- [[ADR-0003 wgpu for graph rendering]]
- [[ADR-0004 WASM components for extensions]]
- [[ADR-0005 Server functions as the remote backend]]
- [[ADR-0006 Native drivers only on native targets]]
- [[ADR-0007 Typst via the native crate]]
- [[ADR-0008 Rust owns the document, JS is a view]]
- [[ADR-0009 Patches not snapshots]]
- [[ADR-0010 dioxus-workbench for layout]]
- [[ADR-0011 Desktop graph surface strategy]] — proposed: in-webview canvas, native overlay fallback
- [[ADR-0012 Two histories]] — proposed: git for text, entity log for the graph, checkpoints between them

## Milestones
- [[Milestone 1 - Walking Skeleton]] (plan) → [[Milestone 1 - Implementation Log]] (what happened) ✅
- [[Milestone 2 - Graph Appears]] (plan) → [[Milestone 2 - Implementation Log]] (what happened) ✅
- [[Milestone 3 - Databases and Tools]] (plan) → [[Milestone 3 - Implementation Log]] (what happened) ✅
- [[Milestone 4 - Agents]] (plan) → [[Milestone 4 - Implementation Log]] (what happened) ✅
- [[Milestone 5 - Settings and Writing]] (plan) → [[Milestone 5 - Implementation Log]] (what happened) ✅
- [[Milestone 6 - Scale and Extend]] (plan) → [[Milestone 6 - Implementation Log]] (what happened) ✅
- [[Milestone 7 - Daily Driver]] (plan) → [[Milestone 7 - Implementation Log]] (what happened) ✅
- [[Milestone 8 - Research]] (plan) → [[Milestone 8 - Implementation Log]] (what happened) ✅
- [[Milestone 9 - Second Halves]] (plan) → [[Milestone 9 - Implementation Log]] (what happened) ✅ — incl. the first Android build on a phone
- [[Milestone 10 - Daily Use]] — the specifications as the backlog: 001–019 done ✅
- [[Milestone 11 - Remote]] (plan) → [[Milestone 11 - Implementation Log]] (what happened) ✅ — remote folders over SSH, the desktop as a client of a server, TLS

## Problems & planning
- [[specifications/README|Specifications]] — small numbered requests and bugs (`markdown/specifications/NNN.md`), edited in place when done
- [[Problem Ranking]] — difficulty × risk, ranked
- [[Roadmap]] — phases and order
- [[Problem Log]] — running log; use [[Problem Template]]

## Research
- [[Database Backends]] · [[Graph Rendering Options]] · [[Rust-native Editor Candidates]] · [[WASM Extension Runtimes]] · [[Versioning Prior Art]] · [[dioxus-flow]]

## Contributing
- [[Development]] — build, run, test, and how this vault becomes the website

## History
- [[old/rough goal|Rough goal (Lumino/Tauri era)]] · the working prompts behind each design session live in `prompts/` (kept in the vault, not published)
