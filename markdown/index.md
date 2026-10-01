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

> [!tip] **[Download the latest build](https://github.com/MathStruct/Moonkale/releases/latest)** — Linux, Android, Windows and macOS packages. New to Moonkale? Read **[[Getting Started]]** — written for people who use VS Code and Obsidian — then [[Install]]. Something wrong? [[Feedback]] says where it goes.

> [!info] Status — Milestone 17 done (2026-09-29): embedded stores; next: the library refactor
> Folders (local and over SSH) that follow the disk, two code editors with LSP, a rich markdown editor with wiki-links and formulas, a GPU graph view in 2D and 3D, SQLite/DuckDB/Turso/redb/RocksDB/HelixDB/LadybugDB as read-only sources, a terminal, git, search, an entity-log history, presence, and agents (Claude Code on a subscription, or any API) under a policy gate — on Linux, the web and Android. Everything, area by area, with what is missing: **[[Status]]**. What comes next: [[Milestone 18 - Library Refactor]] and the [[Roadmap]].

## Start here

- [[Getting Started]] — Moonkale for people who use VS Code and Obsidian
- [[Status]] — what works today, what is missing
- [[Overview]] — the system in one diagram
- [[Project Structure]] — the crate layout and why each crate exists
- [[Graph-Native Model]] — the one big bet
- [[Writing an Extension]] — because the project is extension-driven
- [[Roadmap]] — what comes next, and [[Milestone 18 - Library Refactor]]

## Sections

| | |
|---|---|
| [[Home\|Vault home]] | full map of contents |
| Architecture | [[Overview]] · [[Project Structure]] · [[Platform Matrix]] · [[Graph-Native Model]] · [[Data Sources]] · [[Projects and Sources]] · [[Internal State]] · [[Security]] · [[Julia and Lenticulum]] · [[Publishing Sources]] · [[Remote and Server Modes]] · [[Annotations]] · [[Indexing]] · [[Extension System]] · [[JS Interop Boundary]] · [[JavaScript Inventory]] · [[LLM and RAG]] · [[Agent Sessions and Profiles]] · [[LSP and Terminal]] · [[Debugging and Logging]] · [[Version Management]] · [[Collaboration]] |
| Editors | [[Code Editor]] · [[Code Editor Implementations]] · [[Markdown and Typst Editor]] · [[Table Editor]] · [[Graph View]] · [[Flow Editor]] · [[Terminal]] · [[Unicode Input]] · [[Case Selector]] · [[Markdown Diagrams and Math]] · [[Notebook Editor]] |
| Extensions | [[Extension Catalogue]] · [[Writing an Extension]] · [[Contribution Points]] · [[Manifest Reference]] · [[Host API Reference]] · [[Publishing and Platforms]] · [[Claude Code Extension]] |
| Decisions | [[ADR-0001 Dioxus instead of Lumino and Tauri]] … [[ADR-0014 One store for internal state]] — all in [[Home#Decisions (ADRs)\|Home]] |
| Platform | [[Platform Matrix]] · [[Linux Desktop Setup]] · [[Core Languages]] · [[Licensing]] |
| Packaging | [[Install]] · [[Two Binaries]] · [[Packaging Overview]] · [[Arch Linux]] · [[NixOS]] · [[Android]] · [[Android Extensions and Bundling]] |
| Testing | [[Testing Strategy]] · [[How to Write Tests]] |
| Research | [[Database Backends]] · [[Graph Rendering Options]] · [[Rust-native Editor Candidates]] · [[WASM Extension Runtimes]] · [[Versioning Prior Art]] · [[dioxus-flow]] |
| Planning and records | [[Status]] · [[Roadmap]] · [[Problem Ranking]] · [[Problem Log]] · [[Audit 2026-09-23]] · [[specifications/README\|Specifications]] · milestones 1–18 in [[Home#Milestones\|Home]] |
| Contributing | [[Development]] — building, running, and how this site is published · [[Feedback]] |

Source: [github.com/MathStruct/Moonkale](https://github.com/MathStruct/Moonkale) · Part of [MathStruct](https://mathstruct.github.io/).
