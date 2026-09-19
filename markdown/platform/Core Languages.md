---
title: "Core languages"
description: The languages Daniel uses and will use — the ones Moonkale ships support for as core extensions — with what each has today (file detection, index, LSP, highlighting, preview) and what a "core language extension" must provide; everything else comes from a marketplace later.
tags: [platform, languages, extensions, reference]
---
From [[Prompt10]] (2026-09-19). This is the list Moonkale is built *for*: these get **core extensions** bundled with every build. Other languages register their extension in a marketplace later ([[Publishing and Platforms]]); the mechanism is the same `language` contribution ([[Contribution Points]]), only the packaging differs.

## The list

| group | languages | why |
|---|---|---|
| programming | **Rust**, **Julia**, **C/C++**, **JavaScript/TypeScript**, **Go**, **Lean 4** | what Daniel writes |
| environments / builds | **Nix**, **Pixi** (`pixi.toml`, TOML with a schema) | how the projects are set up |
| databases | **SQL**, **Cypher**, **HelixQL**, **TypeQL**, **GraphQL** | the sources Moonkale opens ([[Data Sources]]) |
| data | **JSON**, **TOML**, (YAML — used by Quartz and CI, so it stays) | configuration everywhere |
| typesetting | **Markdown**, **Typst** | notes and papers |

"Maybe some more I forgot" → add rows here; the table is the contract.

## What "core support" means, per language
A core language extension provides, in this order of importance:
1. **Detection** — extensions, first line, `Node::language_hint` *(exists for most, see below)*.
2. **Highlighting** — a tree-sitter grammar (wasm asset for the browser, native for desktop/server) + `highlights.scm`, rendered through the editor's decoration channel. **Missing for every language today**: the CodeMirror bundle has no language modes by design (`packages/js/codemirror` — "no language knowledge"; Rust was to own it), and the Rust side never sent highlights. This is the biggest single gap on the list.
3. **Symbols in the graph** — tree-sitter queries → `Symbol` nodes and `Defines`/`References` edges in the index *(Rust and Markdown only today)*.
4. **LSP** — a discovered server with an install hint *(rust-analyzer, gopls, lake serve, typescript-language-server, pyright — no clangd, no LanguageServer.jl, no nil/nixd, no SQL/GraphQL servers yet)*.
5. **Comments / brackets / indentation** rules for the editor.
6. **Preview or evaluation** where it applies: Typst preview *(exists)*, Markdown rich editor *(exists)*, SQL/Cypher/TypeQL/HelixQL/GraphQL as *query dialects* run against an open source *(SQL and Cypher exist; the others are rejected by every source today)*, Julia/Lean REPL sessions (not planned yet).

## Where each one stands (2026-09-19)

| language | detect | highlight | symbols | LSP (discovered) | run / preview |
|---|---|---|---|---|---|
| Rust | `.rs` ✅ | ❌ | ✅ (`tree-sitter-rust`) | rust-analyzer ✅ (+ cargo-check diagnostics) | — |
| Julia | `.jl` ✅ | ❌ | ❌ | ❌ (`LanguageServer.jl`: `julia --project=@lsp -e 'using LanguageServer; runserver()'`) | flow editor targets Lux.jl *(exists)*; no REPL |
| C/C++ | ❌ (`.c .h .cc .cpp .hpp` not mapped) | ❌ | ❌ | ❌ (`clangd`) | — |
| JavaScript/TypeScript | `.js .mjs .ts .mts` ✅ (`.jsx .tsx` ❌) | ❌ | ❌ | typescript-language-server ✅ | — |
| Go | `.go` ✅ | ❌ | ❌ | gopls ✅ | — |
| Lean 4 | `.lean` ✅ | ❌ | ❌ | `lake serve` ✅ | no infoview — Lean's goal state needs the `$/lean/plainGoal` extension; a real Lean extension shows it in a side panel |
| Nix | ❌ (`.nix`) | ❌ | ❌ | ❌ (`nil` or `nixd`) | — |
| Pixi | `pixi.toml` → TOML ✅ | ❌ | ❌ | ❌ (`taplo` with the pixi schema) | — |
| SQL | `.sql` ✅ | ❌ | — | ❌ | ✅ table editor runs it against SQLite/DuckDB/Postgres-later |
| Cypher | ❌ (`.cypher .cql`) | ❌ | — | ❌ | ✅ graph sources (LadybugDB) |
| HelixQL | ❌ (`.hx`) | ❌ | — | ❌ | ❌ ([[002]]) |
| TypeQL | ❌ (`.tql`) | ❌ | — | ❌ | ❌ (TypeDB stub) |
| GraphQL | ❌ (`.graphql .gql`) | ❌ | — | ❌ (`graphql-language-service-cli`) | ❌ — the API source kind in [[Projects and Sources]] is where it would run |
| JSON | `.json` ✅ | ❌ | — | ❌ (`vscode-json-language-server`) | — |
| TOML | `.toml` ✅ | ❌ | — | ❌ (`taplo`) | — |
| YAML | `.yml .yaml` ✅ | ❌ | — | ❌ | — |
| Markdown | `.md` ✅ | (rich editor ✅, source view ❌) | ✅ wiki-links | — | ✅ rich editor, backlinks, graph |
| Typst | `.typ` ✅ | ❌ | ❌ | ❌ (`tinymist`) | ✅ preview |

Highlighting column: ❌ everywhere is one problem, not sixteen — one decoration channel plus one grammar per language.

## How they get bundled
- **Core language extensions** live in `packages/extensions/lang-<id>/` as static Rust crates registered in `ui::default_extensions()` (`core` tier — always on), each carrying its grammar (native `tree-sitter-<x>` crate; the `.wasm` grammar as an asset for the browser) and queries. Bundling all sixteen costs grammar size only (≈ 0.2–1 MB each as wasm); on Android that argues for the lazy asset loading in [[Android Extensions and Bundling]].
- **Marketplace languages** ship the same manifest as a wasm extension (`kind = "language"`, data only — grammar + queries + LSP command), installed under `~/.config/moonkale/extensions/<id>/`. Nothing about a marketplace language is second-class except that it is not in the APK.
- LSP servers are never bundled; they are discovered on `PATH` with an install hint *(exists)*, and later per-language settings for a custom command.

## Order to build (suggested)
1. The highlighting channel (Rust → editor decorations) with Rust and Markdown as the first grammars — it unblocks the whole column.
2. Detection for the unmapped extensions (a one-line change in `Node::language_hint`, then the extension-owned table).
3. LSP discovery for clangd, `nil`/`nixd`, `taplo`, `tinymist`, LanguageServer.jl, the JSON and GraphQL servers.
4. Symbols for Julia, Go, TypeScript, C/C++ (grammars exist; queries are the work).
5. Lean infoview, Julia REPL — real extensions with panels, once the UI contribution model exists.
