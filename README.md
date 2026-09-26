![Moonkale — a wizard tending a kale plant under a full moon](assets/MoonkaleBanner.png)

<p align="center">
  <img src="assets/UIDesktop.png" alt="Moonkale on the desktop: activity bar, Explorer, the vault as a graph, a file in the code editor" width="78%">
  <img src="assets/UIAndroid.jpg" alt="Moonkale on a phone: rich note above a Rust file, the bottom bar" width="20%">
</p>

# Moonkale

**A graph-native code and knowledge editor.**

[![Latest release](https://img.shields.io/github/v/release/MathStruct/Moonkale?label=download&color=4c8dff)](https://github.com/MathStruct/Moonkale/releases/latest)
[![Docs](https://img.shields.io/badge/docs-vault-7bd88f)](https://mathstruct.github.io/Moonkale/)
[![License: MIT](https://img.shields.io/badge/license-MIT-lightgrey)](LICENSE)

> [!WARNING]
> **Moonkale is a prototype.** Expect bugs, missing pieces and significant
> breaking changes: settings, file formats, the extension API and the
> package layout will all change without migration paths. Try it, and
> please don't rely on it yet.

## The idea

Code editors, note-taking apps, database tools and no-code tools are separate
worlds. Moonkale opens folders *and* databases and shows everything as
**one graph**: files, symbols, wiki-links, tables and relations are nodes and
edges, and every editor (code, rich text, table, graph, flow, terminal) is a
view on that graph. Language servers, indexers and LLM agents work on the same
graph through the same interface as the user.

It is written in **Rust** with [Dioxus](https://dioxuslabs.com): one codebase
for desktop, web and Android. It is **extension-driven**: the built-in editors
are extensions with no privileged access.

The long-term goal is a graphical editor for factor graphs
([Lenticulum.jl](https://mathstruct.github.io/Moonkale/architecture/Julia-and-Lenticulum)),
where a diagram stays a diagram instead of becoming code.

## What works today

Tested on Linux (desktop and browser) and on one Android phone:

- **Folders**, local or on another machine over SSH, with an Explorer, quick
  open, a command palette, and find & replace across files.
- **Code editing** with syntax highlighting for the core languages and
  language-server support (diagnostics, hover, go to definition, completion,
  rename). There are two editors: CodeMirror, and a pure-Rust one.
- **Notes**: a WYSIWYG Markdown editor with `[[wiki-links]]`, backlinks, KaTeX
  formulas and front matter, plus a Typst preview.
- **Graph view** in 2D and 3D, GPU-rendered, for a folder's files, links and
  symbols, usable with 100k nodes.
- **Databases**: SQLite, DuckDB (including folders of CSV/Parquet files) and
  LadybugDB (Cypher), as tables and as graphs.
- **Terminal**: stack traces in it become clickable graphs.
- **Git**: changes, diffs, staging, commits and history as a graph.
- **Search**: keyword search, plus meaning-based search when an embedding
  model is configured.
- **Change history**: a record of every edit (yours, an agent's or git's),
  with restore.
- **LLM agents**: Claude Code (on a subscription), Anthropic, OpenAI-compatible
  and Ollama. They read, search, query and edit behind an approval gate. There
  is also an MCP endpoint for external agents.
- **Extensions**: WebAssembly extensions, and a node-based flow editor with a
  Lux.jl block library (off by default).
- **Server mode**: `moonkale-server` serves the same app to a browser or to
  the phone.

Windows and macOS builds are produced but **untested**: nobody on the project
has those machines.

## Planned

- More data sources: Postgres, Turso, TypeDB, HelixDB, FalkorDB, Redis. These
  are only stubs today.
- Projects that combine several sources, and links across them.
- The Julia side, in a separate repository: a Julia package source, a
  ModelingToolkit editor, and the Lenticulum.jl factor-graph editor.
- Automatic refresh of the file tree when files change on disk, and a
  refresh button for sources that aren't watched.
- Shared editing on one server: accounts, presence, merged edits.
- Comments anchored to files, rows and nodes, with GitHub issues as one
  kind of comment. Also Jupyter notebooks and Unicode input (`\int` → ∫).
- Fewer JavaScript dependencies: the terminal and a code editor already have
  Rust replacements.

The design notes behind all of this, one note per decision, are the
[documentation site](https://mathstruct.github.io/Moonkale/).

## Install

Download from the [releases page](https://github.com/MathStruct/Moonkale/releases).
Releases are named `moonkale-YYMMDD-prototype-<commit>`; `<release>` below
stands for that name.

| platform | file | install |
|---|---|---|
| Arch Linux | `<release>-arch-x86_64.pkg.tar.zst` | `sudo pacman -U <file>` |
| Debian 12+ / Ubuntu 22.04+ | `<release>-debian-amd64.deb` | `sudo apt install ./<file>` |
| Nix / NixOS | (from source) | `nix profile install github:MathStruct/Moonkale` |
| Debian/Ubuntu (tarball) | `<release>-linux-x86_64.tar.gz` | unpack, `./bin/moonkale`, or `./install.sh ~/.local` |
| Android (arm64) | `<release>-android-arm64.apk` | sideload, or `adb install -r <file>` |
| Windows / macOS | `.msi` / `.exe` / `.dmg` | **unsigned and untested**, see the install page |
| Server only | container | `docker run -p 8080:8080 -v /your/notes:/data ghcr.io/mathstruct/moonkale-server` |

Details and troubleshooting:
[Install](https://mathstruct.github.io/Moonkale/packaging/Install).
New to it? [Getting Started](https://mathstruct.github.io/Moonkale/Getting-Started)
explains Moonkale for people who use VS Code and Obsidian. To build it
yourself: [Development](markdown/Development.md).

## Feedback

Bugs, ideas and "I cannot get it to work" all welcome: [open an
issue](https://github.com/MathStruct/Moonkale/issues/new/choose). Reports on
Windows and macOS are especially useful, since that's the only testing those
builds get. More in [Feedback](https://mathstruct.github.io/Moonkale/Feedback).

## License

**MIT**, see [LICENSE](LICENSE). Built binaries also contain third-party
software under its own permissive licenses: DuckDB, LadybugDB, wasmtime,
Typst, tree-sitter, ICU4X, CodeMirror, Milkdown, xterm.js, KaTeX and its
fonts (OFL). No GPL-only dependency is included. The full list:
[Licensing](https://mathstruct.github.io/Moonkale/platform/Licensing).

Moonkale is a [MathStruct](https://mathstruct.github.io/) project.
