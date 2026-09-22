---
title: "Extension catalogue — current, planned, core or separate"
description: Every extension Moonkale has and every one it plans, with tier, crate, platform fit (desktop / web / mobile), what each needs from outside (server, binary, network), what should stay core and what belongs in a separate repository — and a critique of how much of the core could become extensions.
tags: [extensions, moc, architecture, reference]
---
From [[Prompt15]] (2026-09-19). **Rule: whoever adds, removes, retiers or moves an extension updates this page in the same change.** Mechanism: [[Extension System]]; writing one: [[Writing an Extension]]; where they come from after install: [[Publishing and Platforms]], [[Android Extensions and Bundling]].

Tiers ([[Extension System]]): **core** — always on, cannot be disabled; **optional** — on by default, can be switched off in the *Extensions* panel; **opt-in** — off until enabled. *Static* = a Rust crate compiled into every build and registered in `ui::default_extensions()`; *wasm* = a module loaded at runtime (JSON ABI v1: commands and agent tools only).

Platform columns: ✅ works · ⚙️ works through the server (the web client's process has no PTY/binaries/drivers; the server has) · ❌ not available · ⏳ compiles, not wired. The phone today has no server behind it, so anything that needs a process, a binary or a network driver is ❌ there until it talks to a hub.

## Current (Milestone 13)

Milestone 14 added the Rust code editor next to CodeMirror ([[Code Editor Implementations]]). Since Milestone 13 an extension can contribute its own **settings** (`Extension::settings`), shown under its row in the Extensions panel: the code editor (wrap), markdown (Rich by default, typography), the terminal (shell), the agent (policy, run on the server). Since Milestone 15 Settings no longer lists extensions at all; it keeps what is nobody's alone — the saved agents ([[Agent Sessions and Profiles]]), search, the *Which extension* selectors (code editor, terminal), keybindings, You.

| id | name | crate (lines) | tier | contributes | desktop | web | mobile | needs from outside |
|---|---|---|---|---|---|---|---|---|
| `explorer` | Explorer | `ui/explorer.rs` | core | side panel: sources, tree, file ops, context menu, drag-move, vcs/presence marks | ✅ | ✅ | ✅ | — |
| `search` | Search | `ui/search.rs` | core | side panel: hybrid search, replace | ✅ | ✅ | ✅ | embeddings need an LLM provider (optional) |
| `settings` | Settings | `ui/settings_panel.rs` | core | panel: saved agents, search, which-extension selectors, keybindings, You (Milestone 15) | ✅ | ✅ | ✅ | — |
| `dev.moonkale.extensions` | Extensions | `ui/extensions_panel.rs` | core | panel + activity-bar entry (Milestone 12): built-ins by tier with toggles and permissions, installed wasm modules | ✅ | ✅ | ✅ | — |
| `dev.moonkale.editor-code` | Code editor | `editors/code` (1 143) | core | editor for every text node; LSP hover/definition/completion/rename/actions/references; presence gutter | ✅ | ✅ (LSP ⚙️) | ✅ (LSP ❌) | LSP servers on `PATH` (desktop) or on the server; CodeMirror bundle |
| `dev.moonkale.editor-code-native` | Code Editor (Rust) | `editors/code-native` (≈ 330) | **opt-in** | the JS-free code editor (Milestone 14): `dioxus-code-editor`, tree-sitter in Rust/wasm for every core language incl. Lean, Nix, Typst; save/reload/Ctrl+S, the caret's word; no LSP/decorations. `editor.implementation` and a toolbar switch pick per document | ✅ | ✅ | ✅ | `dioxus-code` + `arborium` (two crates, MIT) |
| `dev.moonkale.editor-markdown` | Markdown & links | `editors/markdown` (635) | optional | rich (Milkdown) + source editing, backlinks/Links panel, Typst preview | ✅ | ✅ (Typst ⚙️) | ✅ (Typst ⏳: `compile_typst: None`) | Milkdown bundle; the `typst` crate (in-process; server on web) |
| `dev.moonkale.editor-graph` | Graph | `editors/graph` + `graph-render` (819 + 1 883) | optional | the graph panel (2D/3D, wgpu, Barnes–Hut), stack-trace graphs | ✅ (WebKitGTK: GL) | ✅ | ✅ (WebGL2, P-089) | a GPU context in the webview |
| `dev.moonkale.editor-image` | Image viewer | `editors/image` (≈ 300) | optional | viewer for png/jpg/gif/webp/svg/bmp/ico/avif: fit/zoom/pan, SVG source | ✅ | ✅ (bytes via `/api/sources/fetch_bytes`) | ✅ | — |
| `dev.moonkale.editor-table` | Table | `editors/table` (226) | optional | editor for SQL/graph sources: schema, rows, queries | ✅ | ⚙️ | ⏳ (no drivers in the mobile build) | drivers: SQLite / DuckDB / LadybugDB (`sources-sql`, `sources-graph`) |
| `dev.moonkale.editor-terminal` | Terminal | `editors/terminal` (395) + `terminal`, `terminal-pty`, `trace` | optional | terminal panel (xterm), trace → graph | ✅ | ⚙️ | ⚙️ (a server's shell, Milestone 12) | a PTY; xterm bundle |
| `dev.moonkale.editor-terminal-native` | Terminal (Rust) | `editors/terminal-native` (≈ 520) | **opt-in** | the JS-free twin (Milestone 12): a `vt100` screen rendered by Dioxus, keys encoded in Rust, colours, scrollback, Ctrl+click links; same backends. `terminal.implementation` picks, or a chooser asks when both are on | ✅ | ⚙️ | ⚙️ | a PTY (desktop) or a server; `vt100` |
| `dev.moonkale.editor-agent` | Agent | `editors/agent` (1 026 + 330) + `llm` (2 239 + 500) | optional | chat panel, tools under the policy gate, transcripts as pages, MCP for external agents (server); **server sessions** (`agent.on_server`, Milestone 12) that finish without a client; the **`claude-code` provider** (the `claude` CLI on the subscription); **sessions** (Milestone 15): any number at once, each on a saved agent, history in the folder ([[Agent Sessions and Profiles]]) | ✅ | ⚙️ | ⚙️ (server sessions when connected) | an LLM provider: Claude Code / Anthropic / OpenAI-compatible / Ollama / mock, keys via `SecretRef` |
| `dev.moonkale.editor-flow` | Flow editor | `editors/flow` (595) | opt-in | editor for `*.flow.json`: typed ports, block libraries from other extensions | ✅ | ✅ | ✅ | — (`dioxus-flow`) |
| `dev.moonkale.lux` | Lux.jl model assembler | `extensions/lux` (583) | opt-in | a block library for the flow editor + Julia code generation | ✅ | ✅ | ✅ (generate only) | Julia + Lux.jl to *run* the generated model (never from Moonkale yet) |
| `dev.moonkale.git` | Git | `extensions/git` (1 045) + `ext-api/git.rs` | optional | Changes panel, diff panel, commit, history as a graph, vcs marks | ✅ | ⚙️ | ❌ (no `git`) | the `git` binary (feature `cli`) |
| `history` | History | `ui/history.rs` | optional | entity-log panel: events, text at any event, compact, restore | ✅ | ✅ | ✅ | — (`.moonkale/history.jsonl`) |
| `dev.moonkale.example-wordcount` | Word count | `extensions/wordcount` (164) | wasm example | two commands / agent tools | ✅ (wasmtime) | ✅ (browser runtime or server) | ❌ (no runtime yet) | the module file |

Not extensions but wired as platform capabilities (`WorkspaceConfig`): presence (web + desktop hub; mobile `None`), the wasm runtime, secret store, settings store, folder picker. They are candidates for extension status only where a contribution point exists.

## Planned (documented in the vault, not built)

| extension | design | tier | where it should live | needs |
|---|---|---|---|---|
| `lang-rust`, `lang-julia`, `lang-c`, `lang-ts`, `lang-go`, `lang-lean`, `lang-nix`, `lang-pixi`, `lang-sql`, `lang-cypher`, `lang-helixql`, `lang-typeql`, `lang-graphql`, `lang-json`, `lang-toml`, `lang-markdown`, `lang-typst` | [[Core Languages]] | core (data-only: grammar, queries, LSP command) | this repo, `packages/extensions/lang-*` | the highlighting channel first |
| `tabs` (Rust), Typst math + CeTZ fences (Rust via `typst`), `mermaid` (JS, lazy), `tikz` (desktop only: native TeX or TikZJax) — all through a fence-renderer contribution | [[Markdown Diagrams and Math]] | tabs/typst-math: optional; mermaid/tikz: opt-in | tikz in a separate repo (heavy wasm), the rest here | TeX on `PATH` for tikz's native backend |
| Annotations (comments on nodes/ranges, GitHub issues import) | [[Annotations]] | optional | this repo | host tokens for import |
| `notebook` — `.ipynb` viewer/editor (no new deps) + kernel execution behind a `kernel` feature (pure-Rust ZeroMQ; ipykernel, IJulia) | [[Notebook Editor]] | opt-in | this repo; desktop/web/phone view, desktop + server execute | Jupyter kernels installed |
| Unicode input tables | [[Unicode Input]] | core (Julia + Lean tables) | this repo (`ui/assets/unicode`) | the `unicode` contribution point |
| Claude Code — Level 2, the IDE bridge (lock file, WebSocket MCP, `openDiff`) | [[Claude Code Extension]] | opt-in | this repo; Level 3 (the provider) shipped in Milestone 12 inside `moonkale-llm` (feature `claude-code`) | the `claude` binary |
| HelixDB source | [[002]] | opt-in | separate (`moonkale-sources-extra`): git dependency, heavy tree | the published crate |
| Postgres / Turso / Redis / TypeDB sources | [[Data Sources]] | opt-in | separate (`moonkale-sources-extra`) | a running server each |
| Remote folder (SSH), git-host repositories, API (OpenAPI) sources | [[Projects and Sources]] | optional | this repo (sources are core-adjacent) | ssh / tokens |
| Julia REPL, LanguageServer.jl, Lean infoview | [[Core Languages]] | opt-in | `MathStruct/moonkale-julia`, `moonkale-lean` | Julia / Lean toolchains |
| Julia depot read-only source (`Project.toml`/`Manifest.toml` → `~/.julia/packages`), ModelingToolkit Simulink-like editor, **Lenticulum.jl factor-graph editor and live viewer** + `Moonkale.jl` companion | [[Julia and Lenticulum]] | opt-in | `MathStruct/moonkale-julia` — never in the standard web build | Julia; a running Julia session for live state |
| Plain-text tier for huge files, coarse graph tier | [[Case Selector]] | core (backends, not extensions) | this repo | — |
| Remote runtime on the phone for wasm extensions + "Install from URL" | [[Android Extensions and Bundling]] | core mechanism | this repo | — |

## What should be core, what should be a separate repository

**Core (this repository, every build):** Explorer, Search, Settings, History; the code editor, Markdown & links (+ Typst preview), Graph, Table, Terminal, Agent (with the provider library), Git, **the Flow editor** (a generic editor for `*.flow.json` with typed ports — the block libraries are what make it domain-specific, and those can live elsewhere); the core language packs; the Unicode tables. Criterion: everything a person needs to *edit code and notes and look at their data* on the first day, with no toolchain beyond Moonkale itself.

**Separate repositories (under `MathStruct/`):**

| repo | contents | why separate | how it plugs in |
|---|---|---|---|
| `moonkale-julia` | the Lux.jl model assembler (today `packages/extensions/lux`), its block library, later Julia REPL / LanguageServer.jl integration, an MTK/data-pipeline library | Julia-specific audience, its own release cadence (Lux.jl changes), and it *needs a Julia toolchain to be useful* — the flow editor does not | a static Rust crate depending on `moonkale-ext-api` (published or by git tag) that contributes a `flow.library`; compiled into a build by a Cargo feature in the platform crates (below) |
| `moonkale-sources-extra` | Postgres, Turso, Redis/Dragonfly, TypeDB, HelixDB drivers | heavy and foreign dependency trees (git deps, C++), each needs a server to test; the core should build in minutes without them | `Source` implementations behind features; desktop/server opt in |
| `moonkale-lean` | Lean infoview, `lake` integration | toolchain-bound, small audience | same as julia |
| `moonkale-extension-template` | the wordcount wasm example, the guest crate scaffold | it is a teaching artefact, not product | copied by extension authors |

A separate repository does **not** mean "downloadable": static Rust extensions are still compiled into the binary (only wasm ones are loaded at runtime — [[Android Extensions and Bundling]]). The mechanism for external static crates is a Cargo feature per extension in the platform crates and a catalogue assembled *there*, which the core does not have yet (critique, next).

## Critique: how sleek is the core today?

What is good: the boundaries that exist are real — JS behind traits with a protocol per package, `Source` for every backend, `Provider` for every LLM, `Extension` + manifests + tiers for every panel, permissions on the wasm ABI; 82 native tests and 29 browser suites; every crate under 4 000 lines. What is not yet sleek, in order of leverage:

1. **`ui` knows every extension.** `packages/ui/Cargo.toml` depends on all eight editor/extension crates, on `api`, and on the SQL/graph driver crates; `ui::default_extensions()` is the catalogue. So the "core" cannot be built without Lux, git, the agent — and an external crate (`moonkale-julia`) cannot be added without editing `ui`. **Fix**: the catalogue moves to the platform crates (`desktop`, `web`, `mobile`) or a small `distribution` crate that lists extensions behind Cargo features (`julia`, `git`, `agent`, …); `ui` depends only on `ext-api`. This is the single change that makes separate repositories possible.
2. **`ext-api::Workspace` is a 1 752-line object** holding documents, sources, history, presence, cursor, browser-wasm, file ops, settings, restore, compaction. Every extension touches it, so every change risks every extension. **Fix**: services behind the facade (`documents`, `sources`, `history`, `presence`, `extensions`), each its own module with its own tests; the `Workspace` keeps the signals and delegates.
3. **Extension code that lives in the core**: the History panel (`ui/history.rs`) and the git *types* (`ext-api/git.rs`, there because the server needs them) belong to their extensions (`packages/extensions/history`; git types in the git crate behind a `types` feature). The Explorer knows DuckDB/LadybugDB file extensions by calling the driver crates — that should be a **file-opener contribution** (`opens: [".duckdb", ".csv"] → source`) so the core has no driver knowledge.
4. **The server cannot be extended.** `api` (1 728 lines) holds the routes for git, LSP, terminal, Typst, wasm, presence, MCP and the LLM relay; an extension that needs a server half (git, a database driver, Claude Code) has to patch `api`. **Fix**: a server-side contribution point — an extension crate with a `server` feature registers its routes/server functions; `api` becomes the host. Together with 1 this is what lets `moonkale-sources-extra` exist.
5. **The wasm ABI cannot contribute UI**, so every panel must be a static crate compiled into every build. Until the UI contribution model (`ui::Tree`) exists, "sleek core + downloadable extensions" is only true for commands and agent tools. This is the long pole; 1–4 are cheap by comparison and worth doing first.
6. **Performance and bug surface, honestly**: the code paths that hurt are not the extension boundary — they are the whole-document-per-keystroke bridge (P-037), the `eval` handshake rules (P-047 family), and dx not rebuilding non-rsx changes (P-056). Splitting crates does not fix those; fixing P-037 does more for "performant" than any restructuring.

## Proposed steps (when scheduled — a milestone of its own)
1. `distribution`/platform catalogue + `ui` without extension deps + Cargo features per extension (`--no-default-features` builds Explorer/Search/Settings/Code/Markdown/Graph only).
2. History → `packages/extensions/history`; git types → git crate; file-opener contribution replaces the driver calls in the Explorer.
3. Server contribution point; git/LSP/terminal/typst relays move into their crates behind `server` features.
4. `packages/extensions/lux` → `MathStruct/moonkale-julia` (depends on `moonkale-ext-api` by git tag until it is on crates.io); the flow editor stays and becomes `optional`; CI here builds with `--features julia` against the tagged repo.
5. `Workspace` split into services.

## Changelog of this page
- 2026-09-19 — created after Milestone 9 with 13 static + 1 wasm extension.
- 2026-09-20 — `editor-image` added (spec 008); planned rows for the Julia depot source, MTK editor and the Lenticulum editor ([[Julia and Lenticulum]]); markdown fence renderers (tabs, Typst math, Mermaid, TikZ), Annotations, and the Notebook editor.
