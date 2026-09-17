---
title: "Project Structure"
tags: [architecture, rationale]
---
This is the "parallel markdown file" for the Rust skeleton in `packages/`. Every crate exists for one of three reasons: it is a **layer boundary**, a **platform boundary**, or a **swap point**. If a crate is none of those, it should be a module instead.

## Layout

```text
packages/
├─ web/ desktop/ mobile/     entrypoints (existing; routers + platform assets)
├─ ui/                       the shell: workbench, panel registry, command palette
├─ api/                      server: fullstack functions = the remote backend; also `RemoteSource`
│
├─ core/                     moonkale-core         domain model, no I/O            [layer]
├─ ext-api/                  moonkale-ext-api      the extension contract          [layer]
├─ ext-host/                 moonkale-ext-host     load/sandbox/dispatch           [platform]
│
├─ sources/                  moonkale-sources      registry, lifting, remote proxy [layer]
├─ sources-sql/              moonkale-sources-sql  postgres sqlite duckdb turso    [platform: native]
├─ sources-graph/            moonkale-sources-graph typedb ladybug helix falkor   [platform: native]
├─ sources-kv/               moonkale-sources-kv   redis dragonfly                 [platform: native]
├─ project-fs/               moonkale-project-fs   folders; per-platform backends  [platform]
├─ index/                    moonkale-index        tree-sitter, links, embeddings  [layer]
│
├─ llm/                      moonkale-llm          providers, tools, policy        [layer]
├─ lsp/                      moonkale-lsp          protocol client, any transport  [layer]
├─ lsp-local/                moonkale-lsp-local    spawn servers                   [platform: desktop/server]
├─ terminal/                 moonkale-terminal     session model, VT grid          [layer]
├─ terminal-pty/             moonkale-terminal-pty local PTY                       [platform: desktop/server]
│
├─ editors/
│  ├─ code/                  moonkale-editor-code      CodeMirror ⇄ native         [swap point]
│  ├─ markdown/              moonkale-editor-markdown  Milkdown ⇄ native, Typst    [swap point]
│  ├─ table/                 moonkale-editor-table     Rust-native grid
│  ├─ graph/                 moonkale-editor-graph     wgpu 2D/3D
│  ├─ graph-desktop/         moonkale-editor-graph-desktop native overlay surface  [platform: desktop]
│  ├─ flow/                  moonkale-editor-flow      no-code canvas + codegen
│  └─ terminal/              moonkale-editor-terminal  xterm ⇄ native              [swap point]
│
└─ js/                       TypeScript packages, one per dependency
   ├─ codemirror/  milkdown/  xterm/     each: src/index.ts, PROTOCOL.md, dist/

packaging/                   PKGBUILD, .desktop entry  (see markdown/packaging/)
flake.nix                    Nix package + dev shell
```

**Status after Milestone 1** ([[Milestone 1 - Implementation Log]]): `core`, `project-fs`, `sources` (registry), `api`, `ext-api`, `editors/code`, `ui` and the three platform crates contain real code with tests; every other crate is still doc comments. Each implemented crate has a `<crate>.md` next to its `Cargo.toml` with implementation notes. `cargo check --workspace`, `cargo test --workspace`, clippy and fmt pass.

## Why these boundaries

### `core` depends on nothing
It is what WASM extensions see. Anything with I/O, async runtimes, or Dioxus in `core` would leak host assumptions into the extension ABI and make the model un-portable to web. It also keeps the model *testable* with plain unit tests. See [[Graph-Native Model]].

### `ext-api` is a separate crate from `ext-host`
The API is a **contract** with a semver; the host is an **implementation** that changes per platform. Extension authors depend on `ext-api` only. If they were one crate, every host change would look like an API change. See [[Extension System]].

### `sources` (no drivers) vs `sources-{sql,graph,kv}` (drivers)
Database drivers link C libraries and cannot compile to `wasm32`. The web build must still know what a source *is* (to show connection UI, to proxy through the server). So the abstraction, lifting rules and the `RemoteSource` proxy live in `sources`, and the drivers are native-only crates the `api` server enables. See [[ADR-0006 Native drivers only on native targets]] and [[Data Sources]].

### `project-fs` has platform backends *inside* one crate, but `lsp-local` / `terminal-pty` are *separate* crates
The rule from the brief: *"if a feature only runs on one platform, separate it."* Folder access exists on all three platforms with different backends — same feature, different implementation → `cfg`-gated modules in one crate. Spawning a language server or a PTY exists on desktop/server **only** → separate crates, so the web build cannot even accidentally reference them. See [[Platform Matrix]].

### Every editor is its own crate
1. Editors are extensions; a crate per extension is exactly the shape a third-party extension has. Dogfooding.
2. Compile-time: the graph editor pulls `wgpu`; the markdown editor pulls `typst`. Nobody working on the table editor should rebuild those.
3. Swap points: `editors/code/src/backend/{codemirror,native}.rs` is where the TypeScript dependency is confined. See [[JS Interop Boundary]].

### `index` is separate from `sources`
Sources report what *is* (files, rows). The index derives what is *implied* (symbols, links, embeddings). Keeping the derived data in its own crate — and exposing it as just another `Source` — means editors don't care whether an edge came from a foreign key or from tree-sitter. See [[Indexing]].

### `llm` is a peer of the editors, not a feature inside them
LLMs consume the same command bus and source surface that humans and extensions use ([[LLM and RAG]]). Putting it at this layer is what makes "an agent can do anything a user can, gated by policy" true by construction.

### `ui` stays the shell only
`ui` owns the workbench (via [[ADR-0010 dioxus-workbench for layout]]), the panel registry, palette, keybindings, and theming. It contains no editor. The current `EditorWorkbench` dummy will be replaced by "render whatever the `ext-host` registry contributes".

### `api` is the server, and the server is "desktop without a screen"
The same native crates (`sources-*`, `lsp-local`, `terminal-pty`, `index`) run inside `api` to serve web and mobile clients. See [[ADR-0005 Server functions as the remote backend]].

## Naming
Workspace crates are `moonkale-*` to avoid collisions (`core` is a reserved crate name; `index`, `terminal`, `lsp` are taken on crates.io). Directories drop the prefix for brevity. The pre-existing template crates (`ui`, `api`, `web`, `desktop`, `mobile`) keep their names.

## What is *not* a crate (yet)
- **Collaboration / CRDT**: [[ADR-0009 Patches not snapshots]] keeps the door open; no crate until it's needed.
- **Settings / config**: a module in `ui` until it grows.
- **Auth** for the server: a module in `api`.
- **Search UI**: a panel in `ui` over `index`.

## Reading order for a new contributor
`core/src/lib.rs` → `core/src/graph/node.rs` → `core/src/source/mod.rs` → `ext-api/src/lib.rs` → `ext-api/src/host.rs` → one editor's `lib.rs` → its `backend/mod.rs`.
