---
title: "Project Structure"
tags: [architecture, rationale]
---
This is the "parallel markdown file" for the Rust skeleton in `packages/`. Every crate exists for one of three reasons: it is a **layer boundary**, a **platform boundary**, or a **swap point**. If a crate is none of those, it should be a module instead.

## Layout (as built, 2026-10-01)
Lines are Rust lines per crate (`wc -l`); kinds as above. The planned changes to this layout are in [[Milestone 18 - Library Refactor]].

```text
packages/
├─ web/ (403)  desktop/ (948)  mobile/ (201)   entrypoints: platform services + the WorkspaceConfig closures
├─ ui/ (4 981)                 the shell: workbench, rail, menus, palette, Explorer, Search, Settings, Extensions, History panels
│                              and default_extensions() — the catalogue (see "Where reality differs")
├─ api/ (2 811)                server: fullstack functions, auth, MCP, presence hub, relays for git/LSP/terminal/wasm/LLM; RemoteSource
│
├─ core/ (1 593)               moonkale-core          ids, Node/Edge, Source, Query, Transaction, EntityLog     [layer]
├─ ext-api/ (≈ 6 300)          moonkale-ext-api       Extension trait, contributions, Workspace (workspace/: one module per area), settings, flow model, wiki  [layer]
├─ ext-host/ (575)             moonkale-ext-host      wasm runtime (wasmtime)                                  [platform]
├─ ext-abi/                    moonkale-ext-abi       the wasm ABI's JSON types (Milestone 18)                  [layer]
├─ state/                      moonkale-state         the store for Moonkale's own state: interface, redb (M18) [layer]
│
├─ sources/ (≈ 60)             moonkale-sources       the source registry                                     [layer]
├─ sources-sql/ (1 525)        moonkale-sources-sql   SQLite, DuckDB (+ data folders), Turso; statement classifier   [native]
├─ sources-graph/ (≈ 1 360)   moonkale-sources-graph LadybugDB (Linux), HelixDB embedded                [native]
├─ sources-kv/ (≈ 1 110)       moonkale-sources-kv    redb, RocksDB (KvSource, `kv` dialect)                 [native]
├─ project-fs/ (1 423)         moonkale-project-fs    folders, ignore rules, notify watcher, trash            [platform]
├─ index/ (2 254)              moonkale-index         IndexSource: wiki-links, tree-sitter symbols, BM25 + embeddings  [layer]
├─ trace/ (537)                moonkale-trace         stack traces → Frame nodes (TraceSource)                [layer]
├─ typst/ (187)                moonkale-typst         Typst → SVG                                              [layer]
│
├─ llm/ (3 040)                moonkale-llm           providers (Claude Code, Anthropic, OpenAI-compatible, Ollama, mock), agent loop, tools, policy, secrets
├─ llm-types/                  moonkale-llm-types     agent data: messages, Provider trait, sessions, policy classes (M18)
├─ lsp/ (862)                  moonkale-lsp           protocol client, any transport                          [layer]
├─ lsp-local/ (255)            moonkale-lsp-local     spawn + discover servers                                [desktop/server]
├─ terminal/ (168)             moonkale-terminal      session model, links                                    [layer]
├─ terminal-pty/ (198)         moonkale-terminal-pty  local PTY                                               [desktop/server]
├─ remote/ (965)               moonkale-remote        folder over ssh → own server                            [desktop]
│
├─ editors/
│  ├─ code/ (1 505)            CodeMirror backend, LSP features                        [swap point]
│  ├─ code-native/ (313)       dioxus-code-editor (Rust, opt-in)                       [swap point]
│  ├─ markdown/ (987)          Milkdown rich view, Links panel, Typst preview          [swap point]
│  ├─ table/ (242)             query + grid
│  ├─ image/ (359)             image viewer
│  ├─ graph/ (930)             the graph panel (Dioxus host of graph-render)
│  ├─ graph-render/ (2 340)    wgpu renderer + Barnes–Hut layout, built to its own wasm module; no moonkale deps
│  ├─ flow/ (596)              dioxus-flow canvas
│  ├─ terminal/ (441)          xterm view                                             [swap point]
│  ├─ terminal-native/ (634)   vt100 + Dioxus (Rust, opt-in)                           [swap point]
│  └─ agent/ (1 765)           the Agent panel, local and server sessions
├─ extensions/
│  ├─ git/ (1 051)             Changes/diff/commit/history graph over the git CLI
│  ├─ lux/ (583)               Lux.jl block library for the flow editor (opt-in; to move to moonkale-julia)
│  └─ wordcount/ (164)         example wasm module (JSON ABI v1)
│
└─ js/                         TypeScript, one dependency per folder: codemirror/ milkdown/ xterm/ wasm-host/
                               each: src/, PROTOCOL.md, built bundle committed into the Rust crate's assets/

packaging/                     PKGBUILDs, .desktop entry, Debian, release scripts (see markdown/packaging/)
flake.nix                      Nix package + dev shell
tools/check-deps.py            the layering rules below, checked in CI (known violations: tools/deps-allow.txt)
.cargo/config.toml             LBUG_LOCALIZE_BUNDLED_SYMBOLS (P-144)
```

## Crate notes
Implementation detail lives **next to the code**, one `<crate>.md` beside each `Cargo.toml` (decided 2026-10-01). They are part of the Obsidian vault (the vault root is the repository root), so in Obsidian they open as notes; the website does not publish `packages/`, so the links below go to GitHub. Each crate note starts with a link back to its design note here; update it in the same change as the crate. The JS packages' message protocols are in `packages/js/*/PROTOCOL.md`.

| crate | note |
|---|---|
| `packages/api` | [api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/api/api.md) |
| `packages/core` | [core.md](https://github.com/MathStruct/Moonkale/blob/master/packages/core/core.md) |
| `packages/desktop` | [desktop.md](https://github.com/MathStruct/Moonkale/blob/master/packages/desktop/desktop.md) |
| `packages/editors/agent` | [editor-agent.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/agent/editor-agent.md) |
| `packages/editors/code-native` | [editor-code-native.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code-native/editor-code-native.md) |
| `packages/editors/code` | [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) |
| `packages/editors/flow` | [flow.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/flow/flow.md) |
| `packages/editors/graph-render` | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph-render/graph-render.md) |
| `packages/editors/graph` | [graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph/graph.md) |
| `packages/editors/image` | [image.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/image/image.md) |
| `packages/editors/markdown` | [markdown.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/markdown/markdown.md) |
| `packages/editors/table` | [table.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/table/table.md) |
| `packages/editors/terminal-native` | [editor-terminal-native.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/terminal-native/editor-terminal-native.md) |
| `packages/editors/terminal` | [editor-terminal.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/terminal/editor-terminal.md) |
| `packages/ext-api` | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md) |
| `packages/ext-host` | [ext-host.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-host/ext-host.md) |
| `packages/extensions/git` | [git.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/git/git.md) |
| `packages/extensions/lux` | [lux.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/lux/lux.md) |
| `packages/extensions/wordcount` | [wordcount.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/wordcount/wordcount.md) |
| `packages/index` | [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/index/index.md) |
| `packages/js` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/js/README.md) |
| `packages/llm` | [llm.md](https://github.com/MathStruct/Moonkale/blob/master/packages/llm/llm.md) |
| `packages/lsp-local` | [lsp-local.md](https://github.com/MathStruct/Moonkale/blob/master/packages/lsp-local/lsp-local.md) |
| `packages/lsp` | [lsp.md](https://github.com/MathStruct/Moonkale/blob/master/packages/lsp/lsp.md) |
| `packages/mobile` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/mobile/README.md) |
| `packages/project-fs` | [project-fs.md](https://github.com/MathStruct/Moonkale/blob/master/packages/project-fs/project-fs.md) |
| `packages/remote` | [remote.md](https://github.com/MathStruct/Moonkale/blob/master/packages/remote/remote.md) |
| `packages/sources-graph` | [sources-graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-graph/sources-graph.md) |
| `packages/sources-kv` | [sources-kv.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-kv/sources-kv.md) |
| `packages/sources-sql` | [sources-sql.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-sql/sources-sql.md) |
| `packages/sources` | [sources.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/sources.md) |
| `packages/terminal-pty` | [terminal-pty.md](https://github.com/MathStruct/Moonkale/blob/master/packages/terminal-pty/terminal-pty.md) |
| `packages/terminal` | [terminal.md](https://github.com/MathStruct/Moonkale/blob/master/packages/terminal/terminal.md) |
| `packages/trace` | [trace.md](https://github.com/MathStruct/Moonkale/blob/master/packages/trace/trace.md) |
| `packages/typst` | [typst.md](https://github.com/MathStruct/Moonkale/blob/master/packages/typst/typst.md) |
| `packages/ui` | [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| `packages/web/tests/e2e` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/web/tests/e2e/README.md) |

## Where reality differs from the rules below
Measured 2026-10-01; each item is a step of [[Milestone 18 - Library Refactor]].
- **`ui` is not "the shell only"**: it depends on every editor and extension crate and on `api`, and `ui::default_extensions()` is the catalogue — so the core cannot be built without them and an outside crate cannot be added without editing `ui`. (Its driver dependencies are gone since phase 2: the Explorer asks the source openers.)
- **`ext-api` is not yet a small contract**: since phase 3 it depends only on `core`, `ext-abi`, `llm-types`, `lsp` and `terminal`, and `Workspace` (2 603 lines, 39 public signals, 94 public methods) holds the state of almost every feature.
- ~~`llm` depends on `sources-sql`~~ — gone in phase 2: sources classify their own queries (`Source::classify`).
- **`api` depends on the git extension**, and holds the server half of every extension that has one.
- ~~Opening a database file is decided in four places~~ — one `Openers` list since phase 2.
- ~~21 files and one crate were design stubs~~ — removed in Milestone 18 phase 1 (24 files with three orphans nobody declared, and `editors/graph-desktop`); their designs moved to [[Data Sources]], [[Code Editor]], [[ADR-0011 Desktop graph surface strategy]].

## Why these boundaries

### `core` depends on nothing
It is what WASM extensions see. Anything with I/O, async runtimes, or Dioxus in `core` would leak host assumptions into the extension ABI and make the model un-portable to web. It also keeps the model *testable* with plain unit tests. See [[Graph-Native Model]].

### `ext-api` is a separate crate from `ext-host`
The API is a **contract** with a semver; the host is an **implementation** that changes per platform. Extension authors depend on `ext-api` only. If they were one crate, every host change would look like an API change. See [[Extension System]].

### `sources` (no drivers) vs `sources-{sql,graph,kv}` (drivers)
Database drivers link C libraries and cannot compile to `wasm32`. The web build must still know what a source *is* (to show connection UI, to proxy through the server). So the abstraction, lifting rules and the `RemoteSource` proxy were to live in `sources`, and the drivers are native-only crates the `api` server enables. *As built*: `RemoteSource` lives in `api` (to avoid a cycle), the lifting rules were never written (each driver lifts its own rows), and `sources` is a registry. See [[ADR-0006 Native drivers only on native targets]] and [[Data Sources]].

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
`ui` owns the workbench (via [[ADR-0010 dioxus-workbench for layout]]), the panel registry, palette, keybindings, and theming. It contains no editor — but it does hold the catalogue and five core panels today (see above).

### `api` is the server, and the server is "desktop without a screen"
The same native crates (`sources-*`, `lsp-local`, `terminal-pty`, `index`) run inside `api` to serve web and mobile clients. See [[ADR-0005 Server functions as the remote backend]].

## Naming
Workspace crates are `moonkale-*` to avoid collisions (`core` is a reserved crate name; `index`, `terminal`, `lsp` are taken on crates.io). Directories drop the prefix for brevity. The pre-existing template crates (`ui`, `api`, `web`, `desktop`, `mobile`) keep their names.

## What is *not* a crate (yet)
- **Collaboration / CRDT**: [[ADR-0009 Patches not snapshots]] keeps the door open; no crate until it's needed. Presence lives in `api::presence` + `ext-api::presence`.
- **Settings / config**: typed `Settings` in `ext-api::settings` with user and workspace scopes; stored per platform (built in Milestone 5). One store for all internal state is proposed: [[Internal State]].
- **Auth** for the server: `api::auth` (token, rate limit, bind guard).
- **Search UI**: a panel in `ui` over `index`.
- **Git**: not a `vcs-git` crate as first planned, but the `extensions/git` extension over the `git` CLI.

## Reading order for a new contributor
`core/src/lib.rs` → `core/src/graph/node.rs` → `core/src/source/mod.rs` (the `Source` trait) → `ext-api/src/extension.rs` (the `Extension` trait) → `ext-api/src/workspace.rs` (the host handle; long) → `ui/src/lib.rs` (`default_extensions`) → one editor's `extension.rs` and `panel.rs` → `editors/code/src/backend/mod.rs` (the JS swap point). Each crate's `<crate>.md` next to its `Cargo.toml` has the implementation notes.
