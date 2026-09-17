---
tags: [architecture]
---
# Overview

Moonkale unifies code editing and knowledge editing around one idea: **everything you open becomes a graph**, and every editor is a view on that graph.

```mermaid
flowchart LR
  subgraph Sources
    FS[Folder]
    SQL[(Postgres / SQLite / DuckDB / Turso)]
    GDB[(TypeDB / Ladybug / Helix / Falkor)]
    KV[(Redis / Dragonfly)]
  end
  subgraph Core["moonkale-core: graph model"]
    G[(Nodes + Edges)]
    Q[Query IR]
  end
  subgraph Derived
    IDX[Index: tree-sitter, links, embeddings]
    LSP[LSP client]
  end
  subgraph Editors
    CODE[Code]
    MD[Markdown / Typst]
    TBL[Table / SQL]
    GRAPH[Graph 2D/3D wgpu]
    FLOW[Flow / no-code]
    TERM[Terminal]
  end
  subgraph Agents
    LLM[LLM gateway + policy]
  end
  FS & SQL & GDB & KV -->|Source trait| G
  G --> IDX --> G
  LSP --> IDX
  G -->|GraphView| CODE & MD & TBL & GRAPH & FLOW
  TERM -->|links| IDX
  LLM <-->|tools| G
  EXT[Extensions: static + WASM] -.contribute.-> Editors & Sources & LLM
```

## The three bets

1. **Graph-native model** ([[Graph-Native Model]]). A folder, a Postgres schema and an Obsidian vault are all nodes and edges. This is what lets one graph view show a Rust crate next to a database next to a wiki, and what gives LLM agents a single traversal surface.
2. **Extension-driven** ([[Extension System]]). The built-in editors are extensions with no privileged access. If the API can't express the code editor, the API is wrong.
3. **Rust everywhere, GPU where it matters** ([[ADR-0002 Rust first, TypeScript behind traits]], [[ADR-0003 wgpu for graph rendering]]). One language across desktop/web/mobile via Dioxus; the graph view is `wgpu` so it stays fluid where DOM-based tools (Obsidian, Cytoscape) fall over.

## Layers

```mermaid
flowchart TB
  A[web / desktop / mobile entrypoints] --> B[ui — workbench shell]
  B --> C[editors/*]
  C --> D[ext-api]
  D --> E[core]
  B --> F[ext-host] --> D
  G[sources, sources-sql/graph/kv, project-fs] --> E
  H[index, lsp, terminal, llm] --> E
  I[api — server: fullstack functions] --> G & H
```

Dependency direction is strictly downward. `core` depends on nothing. See [[Project Structure]] for every crate.

## What this replaces

The earlier plan ([[old/rough goal]]) was Deno + Vite + Lumino on the frontend and Tauri + Rust on the backend. [[ADR-0001 Dioxus instead of Lumino and Tauri]] explains the move to a single Rust codebase with Dioxus 0.7 and why the TypeScript pieces that survive (CodeMirror, Milkdown, xterm) are quarantined behind traits ([[JS Interop Boundary]]).
