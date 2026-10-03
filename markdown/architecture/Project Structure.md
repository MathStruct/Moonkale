---
title: "Project Structure"
tags: [architecture, rationale]
---
This is the "parallel markdown file" for the Rust skeleton in `packages/`. Every crate exists for one of three reasons: it is a **layer boundary**, a **platform boundary**, or a **swap point**. If a crate is none of those, it should be a module instead.

## Layout (as built, 2026-10-03, after the renames of Milestone 18 phase 6.5)
Lines are Rust lines per crate; kinds as above. Every move is in the rename table of [[Milestone 18 - Implementation Log]] (phase 6.5).

```text
packages/
├─ web/ (536)  desktop/ (1 018)  mobile/ (281)   apps: platform services + the WorkspaceConfig closures
├─ distribution/ (49)          moonkale-distribution  which extensions an app ships: one Cargo feature each           [assembly]
├─ shell/ (4 755)              moonkale-shell         the workbench: rail, menus, palette, Explorer, Search, Settings, Extensions; theme.css
├─ server/ (3 248)             moonkale-server        the server host: sources, auth, MCP, presence hub, relays (terminal/LSP/wasm/LLM), agent sessions, the folder host's store; the client side (RemoteSource, client::*)
├─ server-host/ (39)           moonkale-server-host   what every server half shares: the jail                       [layer]
│
├─ core/ (2 055)               moonkale-core          ids, Node/Edge (+ properties), Source, Query, Transaction, EntityLog   [library]
├─ ext-api/ (7 709)            moonkale-ext-api       the contract: Extension, contributions, Workspace, settings, i18n; CHANGELOG.md   [library]
├─ graph-render/ (2 643)       moonkale-graph-render  layout + camera + wgpu renderer; `scene` (graph in, events out); no Moonkale deps   [library]
├─ ext-abi/ (108)              moonkale-ext-abi       the wasm ABI's JSON types                                       [layer]
├─ ext-host/ (514)             moonkale-ext-host      wasm runtime (wasmtime)                                        [platform]
├─ state/ (758)                moonkale-state         the store for Moonkale's own state: the interface               [layer]
├─ state-stores/ (1 152)       moonkale-state-stores  its engines: SQLite (the app's), redb, Turso, RocksDB, Helix     [native]
├─ code-view/ (1 403)          moonkale-code-view     the CodeMirror component + LSP manager (used by Code and Markdown) [component]
│
├─ sources/
│  ├─ registry/ (69)           moonkale-sources       the source registry                                            [layer]
│  ├─ sql/ (1 484)             moonkale-sources-sql   SQLite, DuckDB (+ data folders), Turso                         [native]
│  ├─ graph/ (1 397)           moonkale-sources-graph LadybugDB (Linux), HelixDB embedded                            [native]
│  ├─ kv/ (1 143)              moonkale-sources-kv    redb, RocksDB                                                  [native]
│  └─ project-fs/ (1 424)      moonkale-project-fs    folders, ignore rules, watcher, trash                          [platform]
├─ services/
│  ├─ llm/ (2 751)             moonkale-llm           providers, agent loop, tools, policy, secrets
│  ├─ llm-types/ (320)         moonkale-llm-types     agent data: messages, Provider trait, sessions
│  ├─ lsp/ (862)               moonkale-lsp           protocol client, any transport                                 [layer]
│  ├─ lsp-local/ (255)         moonkale-lsp-local     spawn + discover servers                                       [desktop/server]
│  ├─ terminal/ (168)          moonkale-terminal      session model, links                                           [layer]
│  ├─ terminal-pty/ (198)      moonkale-terminal-pty  local PTY                                                      [desktop/server]
│  ├─ remote/ (971)            moonkale-remote        folder over ssh → own server                                   [desktop]
│  ├─ typst/ (187)             moonkale-typst         Typst → SVG
│  ├─ index/ (2 321)           moonkale-index         IndexSource: wiki-links, symbols, BM25 + embeddings
│  └─ trace/ (539)             moonkale-trace         stack traces → Frame nodes
│
├─ editors/
│  ├─ code/ (115)              the Code extension (over code-view)                    [swap point]
│  ├─ code-native/ (302)       dioxus-code-editor (Rust, opt-in)                      [swap point]
│  ├─ markdown/ (1 028)        Milkdown rich view, Links panel, Typst preview; server half: Typst
│  ├─ table/ (245)  image/ (362)  graph/ (933)  flow/ (601)
│  ├─ terminal/ (439)          xterm view                                             [swap point]
│  ├─ terminal-native/ (630)   vt100 + Dioxus (Rust, opt-in)                          [swap point]
│  └─ agent/ (2 020)           the Agent panel, local and server sessions
├─ extensions/
│  ├─ git/ (1 390)             Changes/diff/commit/history graph; server half: git_run
│  ├─ history/ (264)           the History panel
│  └─ wordcount/ (164)         example wasm module (JSON ABI v1)
│                              (Lux.jl: MathStruct/moonkale-julia, feature `julia` — phase 6.4)
│
└─ js/                         TypeScript, one dependency per folder: codemirror/ milkdown/ xterm/ wasm-host/

packaging/                     PKGBUILDs, .desktop entry, Debian, release scripts (see markdown/packaging/)
flake.nix                      Nix package + dev shell
tools/check-deps.py            the layering rules below, checked in CI (no known violations since phase 4.3)
tools/check-colors.py          hard-coded colours outside theme.css may only shrink (phase 4.4)
.cargo/config.toml             LBUG_LOCALIZE_BUNDLED_SYMBOLS (P-144)
```

The apps stay `packages/{web,desktop,mobile}` — not `apps/` as the plan proposed: their paths are in the packaging scripts, the release workflow, the Nix flake, the PKGBUILD, the Android build and the E2E harness, and a release-only breakage would show only at the next tag (phase 6.5, decided then).

## Crate notes
Implementation detail lives **next to the code**, one `<crate>.md` beside each `Cargo.toml` (decided 2026-10-01). They are part of the Obsidian vault (the vault root is the repository root), so in Obsidian they open as notes; the website does not publish `packages/`, so the links below go to GitHub. Each crate note starts with a link back to its design note here; update it in the same change as the crate. The JS packages' message protocols are in `packages/js/*/PROTOCOL.md`.

| crate | note |
|---|---|
| `packages/code-view` | in [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) (phase 4.1) |
| `packages/core` | [core.md](https://github.com/MathStruct/Moonkale/blob/master/packages/core/core.md) |
| `packages/desktop` | [desktop.md](https://github.com/MathStruct/Moonkale/blob/master/packages/desktop/desktop.md) |
| `packages/distribution` | [distribution.md](https://github.com/MathStruct/Moonkale/blob/master/packages/distribution/distribution.md) |
| `packages/editors/agent` | [editor-agent.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/agent/editor-agent.md) |
| `packages/editors/code-native` | [editor-code-native.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code-native/editor-code-native.md) |
| `packages/editors/code` | [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) |
| `packages/editors/flow` | [flow.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/flow/flow.md) |
| `packages/editors/graph` | [graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph/graph.md) |
| `packages/editors/image` | [image.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/image/image.md) |
| `packages/editors/markdown` | [markdown.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/markdown/markdown.md) |
| `packages/editors/table` | [table.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/table/table.md) |
| `packages/editors/terminal-native` | [editor-terminal-native.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/terminal-native/editor-terminal-native.md) |
| `packages/editors/terminal` | [editor-terminal.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/terminal/editor-terminal.md) |
| `packages/ext-api` | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md) |
| `packages/ext-host` | [ext-host.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-host/ext-host.md) |
| `packages/extensions/git` | [git.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/git/git.md) |
| `packages/extensions/history` | [history.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/history/history.md) |
| `packages/extensions/wordcount` | [wordcount.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/wordcount/wordcount.md) |
| `packages/graph-render` | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/graph-render/graph-render.md) |
| `packages/js` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/js/README.md) |
| `packages/mobile` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/mobile/README.md) |
| `packages/server-host` | [server-host.md](https://github.com/MathStruct/Moonkale/blob/master/packages/server-host/server-host.md) |
| `packages/server` | [server.md](https://github.com/MathStruct/Moonkale/blob/master/packages/server/server.md) |
| `packages/services/index` | [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/index/index.md) |
| `packages/services/llm` | [llm.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/llm/llm.md) |
| `packages/services/lsp-local` | [lsp-local.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/lsp-local/lsp-local.md) |
| `packages/services/lsp` | [lsp.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/lsp/lsp.md) |
| `packages/services/remote` | [remote.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/remote/remote.md) |
| `packages/services/terminal-pty` | [terminal-pty.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/terminal-pty/terminal-pty.md) |
| `packages/services/terminal` | [terminal.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/terminal/terminal.md) |
| `packages/services/trace` | [trace.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/trace/trace.md) |
| `packages/services/typst` | [typst.md](https://github.com/MathStruct/Moonkale/blob/master/packages/services/typst/typst.md) |
| `packages/shell` | [shell.md](https://github.com/MathStruct/Moonkale/blob/master/packages/shell/shell.md) |
| `packages/sources/graph` | [sources-graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/graph/sources-graph.md) |
| `packages/sources/kv` | [sources-kv.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/kv/sources-kv.md) |
| `packages/sources/project-fs` | [project-fs.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/project-fs/project-fs.md) |
| `packages/sources/registry` | [sources.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/registry/sources.md) |
| `packages/sources/sql` | [sources-sql.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/sql/sources-sql.md) |
| `packages/state-stores` | [state-stores.md](https://github.com/MathStruct/Moonkale/blob/master/packages/state-stores/state-stores.md) |
| `packages/state` | [state.md](https://github.com/MathStruct/Moonkale/blob/master/packages/state/state.md) |
| `packages/web` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/web/README.md) |
| `packages/web/tests/e2e` | [README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/web/tests/e2e/README.md) |

## Where reality differs from the rules below
Measured 2026-10-01; each item is a step of [[Milestone 18 - Library Refactor]].
- ~~`ui` is not "the shell only"~~ — since phase 4.1 the catalogue is `moonkale-distribution` (a Cargo feature per extension) and `ui` depends on `core`, `ext-api` and the protocol crates only; History is its own extension (4.2), and the CodeMirror component is `code-view`, so Markdown no longer depends on the Code extension.
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
- **Collaboration / CRDT**: [[ADR-0009 Patches not snapshots]] keeps the door open; no crate until it's needed. Presence lives in `moonkale_server::presence` + `ext-api::presence`.
- **Settings / config**: typed `Settings` in `ext-api::settings` with user and workspace scopes; stored per platform (built in Milestone 5). One store for all internal state is proposed: [[Internal State]].
- **Auth** for the server: `moonkale_server::auth` (token, rate limit, bind guard).
- **Search UI**: a panel in `ui` over `index`.
- **Git**: not a `vcs-git` crate as first planned, but the `extensions/git` extension over the `git` CLI.

## Reading order for a new contributor
`core/src/lib.rs` → `core/src/graph/node.rs` → `core/src/source/mod.rs` (the `Source` trait) → `ext-api/src/extension.rs` (the `Extension` trait) → `ext-api/src/workspace.rs` (the host handle; long) → `shell/src/lib.rs` (`default_extensions`) → one editor's `extension.rs` and `panel.rs` → `editors/code/src/backend/mod.rs` (the JS swap point). Each crate's `<crate>.md` next to its `Cargo.toml` has the implementation notes.
