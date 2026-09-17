---
title: Moonkale
description: A graph-native code and knowledge editor — folders and databases as one graph, extension-driven, built in Rust with Dioxus.
---

**Moonkale** is a code and knowledge editor built on one idea: *everything you open becomes a graph*. A folder of source files, a Postgres schema, a TypeDB database, a Redis keyspace and an Obsidian-style wiki all become nodes and edges in the same model — and every editor (code, markdown/Typst, tables, a GPU graph view, a no-code flow canvas, a terminal) is a view on that graph. Language servers, indexers and LLM agents work on the same graph through the same doors.

It is written in Rust with [Dioxus](https://dioxuslabs.com), targets desktop, web and mobile from one codebase, and is extension-driven: the built-in editors are themselves extensions with no privileged access.

> [!info] Status — Milestone 1 done
> The **walking skeleton** works: open a folder, browse it, open files in dockable CodeMirror tabs, edit, save (with conflict detection) — on the web build against a server-side folder, and compiled for desktop/mobile with an in-process folder source. Everything else in the architecture is still a comment-only skeleton. See [[Milestone 1 - Implementation Log]], then [[Roadmap]] for what comes next.

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
| Architecture | [[Overview]] · [[Platform Matrix]] · [[Data Sources]] · [[Indexing]] · [[Extension System]] · [[JS Interop Boundary]] · [[LLM and RAG]] · [[LSP and Terminal]] · [[Debugging and Logging]] · [[Version Management]] |
| Editors | [[Code Editor]] · [[Markdown and Typst Editor]] · [[Table Editor]] · [[Graph View]] · [[Flow Editor]] · [[Terminal]] |
| Extensions | [[Writing an Extension]] · [[Contribution Points]] · [[Manifest Reference]] · [[Host API Reference]] |
| Decisions | [[ADR-0001 Dioxus instead of Lumino and Tauri]] … [[ADR-0011 Desktop graph surface strategy]] |
| Platform | [[Linux Desktop Setup]] |
| Packaging | [[Packaging Overview]] · [[Arch Linux]] · [[NixOS]] · [[Android]] |
| Testing | [[Testing Strategy]] · [[How to Write Tests]] |
| Research | [[Database Backends]] · [[Graph Rendering Options]] · [[Rust-native Editor Candidates]] · [[WASM Extension Runtimes]] · [[Versioning Prior Art]] |
| Milestones | [[Milestone 1 - Walking Skeleton]] · [[Milestone 1 - Implementation Log]] |
| Contributing | [[Development]] — building, running, and how this site is published |

Source: [github.com/MathStruct/Moonkale](https://github.com/MathStruct/Moonkale) · Part of [MathStruct](https://mathstruct.github.io/).
