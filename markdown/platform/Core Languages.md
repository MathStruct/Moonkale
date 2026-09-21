---
title: "Core languages"
description: The languages Daniel uses and will use — the ones Moonkale ships support for as core extensions — with what each has today (file detection, index, LSP, highlighting, preview) and what a "core language extension" must provide; everything else comes from a marketplace later.
tags: [platform, languages, extensions, reference]
---
From [[Prompt10]] (2026-09-19). This is the list Moonkale is built *for*: these get **core extensions** bundled with every build. Other languages register their extension in a marketplace later ([[Publishing and Platforms]]); the mechanism is the same `language` contribution ([[Contribution Points]]), only the packaging differs.

## The list

| group | languages | why |
|---|---|---|
| programming | **Rust**, **Julia**, **Python**, **C/C++**, **JavaScript/TypeScript**, **Go**, **Lean 4** | what Daniel writes (Python added 2026-09-20, [[Prompt19]]) |
| environments / builds | **Nix**, **Pixi** (`pixi.toml`, TOML with a schema) | how the projects are set up |
| databases | **SQL**, **Cypher**, **HelixQL**, **TypeQL**, **GraphQL** | the sources Moonkale opens ([[Data Sources]]) |
| data | **JSON**, **TOML**, (YAML — used by Quartz and CI, so it stays) | configuration everywhere |
| typesetting | **Markdown** (with Typst or KaTeX math, TikZ, Mermaid, tabs — [[Markdown Diagrams and Math]]), **Typst** | notes and papers |

"Maybe some more I forgot" → add rows here; the table is the contract.

## What "core support" means, per language
A core language extension provides, in this order of importance:
1. **Detection** — extensions, first line, `Node::language_hint` *(exists for most, see below)*.
2. **Highlighting** — today a Lezer/legacy grammar in the CodeMirror bundle (P-093, spec [[010]]); tree-sitter + `highlights.scm` through Rust remains the design for the Rust-native editor.
3. **Symbols in the graph** — tree-sitter queries → `Symbol` nodes and `Defines`/`References` edges in the index *(Rust and Markdown only today)*.
4. **LSP** — a discovered server with an install hint *(rust-analyzer, gopls, lake serve, typescript-language-server, pyright — no clangd, no LanguageServer.jl, no nil/nixd, no SQL/GraphQL servers yet)*.
5. **Comments / brackets / indentation** rules for the editor.
6. **Preview or evaluation** where it applies: Typst preview *(exists)*, Markdown rich editor *(exists)*, SQL/Cypher/TypeQL/HelixQL/GraphQL as *query dialects* run against an open source *(SQL and Cypher exist; the others are rejected by every source today)*, Julia/Lean REPL sessions (not planned yet).

## Where each one stands (2026-09-19)

| language | detect | highlight | symbols | LSP (discovered) | run / preview |
|---|---|---|---|---|---|
| Rust | `.rs` ✅ | ✅ Lezer | ✅ (`tree-sitter-rust`) | rust-analyzer ✅ (+ cargo-check diagnostics) | — |
| Python | `.py` ✅ | ✅ Lezer | ✅ `def`/`class` (spec 022) | pyright / pylsp ✅ | `.ipynb` viewer/editor + ipykernel execution planned as an opt-in extension ([[Notebook Editor]]) |
| Julia | `.jl` ✅ | ✅ legacy mode | ✅ functions, structs, modules, macros, consts (spec 022) | ✅ discovered (`julia --project=@lsp -e 'using LanguageServer; runserver()'`, install hint) | flow editor targets Lux.jl *(exists)*; no REPL |
| C/C++ | ✅ `.c .h .cc .cpp .cxx .hh .hpp .hxx .cu` | ✅ Lezer | ❌ | ✅ clangd discovered | — |
| JavaScript/TypeScript | ✅ `.js .mjs .cjs .jsx .ts .mts .cts .tsx` | ✅ Lezer (JSX) | ❌ | typescript-language-server ✅ | — |
| Go | `.go` ✅ | ✅ Lezer | ❌ | gopls ✅ | — |
| Lean 4 | `.lean` ✅ | ❌ (no grammar on npm) | ❌ | `lake serve` ✅ | no infoview — Lean's goal state needs the `$/lean/plainGoal` extension; a real Lean extension shows it in a side panel |
| Nix | ✅ `.nix` | ❌ (no grammar on npm) | ❌ | ✅ nil / nixd discovered | — |
| Pixi | `pixi.toml` → TOML ✅ | ✅ (TOML) | ❌ | ✅ taplo discovered (schema later) | — |
| SQL | `.sql` ✅ | ✅ Lezer (SQLite dialect) | — | ❌ | ✅ table editor runs it against SQLite/DuckDB/Postgres-later |
| Cypher | ✅ `.cypher .cql` | ✅ legacy mode | — | ❌ | ✅ graph sources (LadybugDB) |
| HelixQL | ✅ `.hx` | ❌ | — | ❌ | ❌ ([[002]]) |
| TypeQL | ✅ `.tql` | ❌ | — | ❌ | ❌ (TypeDB stub) |
| GraphQL | ✅ `.graphql .gql` | ❌ (`cm6-graphql` drags in 500 KB of `graphql`) | — | ✅ graphql-lsp discovered | ❌ — the API source kind in [[Projects and Sources]] is where it would run |
| JSON | `.json .jsonc .ipynb` ✅ | ✅ Lezer | — | ✅ vscode-json-language-server discovered | — |
| TOML | `.toml` ✅ | ✅ legacy mode | — | ✅ taplo discovered | — |
| YAML | `.yml .yaml` ✅ | ✅ Lezer | — | ✅ yaml-language-server discovered | — |
| Markdown | `.md` ✅ | ✅ rich editor + Lezer in source view | ✅ wiki-links ([[012]]: completion, follow/create, rename rewrites) | — | ✅ rich editor with KaTeX ([[013]]), backlinks, graph |
| Typst | `.typ` ✅ | ✅ Lezer (`codemirror-lang-typst`) | ❌ | ✅ tinymist discovered | ✅ preview |

Highlighting (2026-09-20, [[010]]): grammars ship in the CodeMirror bundle — decision P-093 — so every language with a Lezer or legacy grammar highlights; Lean, Nix, HelixQL, TypeQL and GraphQL wait for one. "LSP discovered" means the server is started when it is on `PATH`, with an install hint otherwise.

## How they get bundled
- **Core language extensions** live in `packages/extensions/lang-<id>/` as static Rust crates registered in `ui::default_extensions()` (`core` tier — always on), each carrying its grammar (native `tree-sitter-<x>` crate; the `.wasm` grammar as an asset for the browser) and queries. Bundling all sixteen costs grammar size only (≈ 0.2–1 MB each as wasm); on Android that argues for the lazy asset loading in [[Android Extensions and Bundling]].
- **Marketplace languages** ship the same manifest as a wasm extension (`kind = "language"`, data only — grammar + queries + LSP command), installed under `~/.config/moonkale/extensions/<id>/`. Nothing about a marketplace language is second-class except that it is not in the APK.
- LSP servers are never bundled; they are discovered on `PATH` with an install hint *(exists)*, and later per-language settings for a custom command.

## Order to build (suggested)
1. ~~Highlighting~~ done via the bundle (P-093); Lean/Nix grammars when one appears.
2. ~~Detection~~ done for the whole list.
3. ~~LSP discovery~~ done; per-language custom commands in settings next.
4. Symbols for Go, TypeScript, C/C++, Lean, Nix (tree-sitter grammars exist; Julia and Python were done in spec 022 with a walker each — ~150 lines per language); references and calls for any language.
5. Lean infoview, Julia REPL — real extensions with panels, once the UI contribution model exists.
