# Moonkale (Under Development)

## Introduction

Currently, tooling for editing both code and knowledge is fragmented:

* **Code editors**: VSCode, JetBrains, [Zed](https://zed.dev/), etc.
* **Knowledge editors**: [Obsidian](Obsidian.md), Logseq, Notion, ... as well as [Wikis](https://golem.ph.utexas.edu/wiki/instiki/show/HomePage)
* Most editors open folders in the file system, but it would often be more beneficial to work directly with structured data, such as from **(graph) databases**.
* **No-code/low-code/diagrams** are rarely a natural extension of such editors.
* Not built with **Retrieval Augmented Generation (RAG)** or [Unison](https://www.unison-lang.org/docs/the-big-idea/)-like compilers in mind.

Moonkale is an attempt to unify **code + knowledge editing** in a lightweight, extensible, and performant system.

---

## Architecture

### Frontend

* **[Deno](https://docs.deno.com/runtime/) + [Vite](https://vite.dev/guide/)** powers the frontend, keeping the stack lightweight and TypeScript-native.
* [**Lumino**](https://lumino.readthedocs.io/en/latest/api/index.html) owns the DOM for flexible, IDE-like layouts.
* Planned components:
  * [ ] [**Cytoscape**](https://js.cytoscape.org/) – Graph visualization and interaction.
  * [ ] [**Milkdown**](https://milkdown.dev/docs/guide/getting-started) – Markdown editing for knowledge management.
  * [ ] [**CodeMirror**](https://codemirror.net/docs/) – Code editing.
  * and many more ...

### Backend

* **[Tauri](https://tauri.app/start/) + Rust** provides a performant backend while keeping the frontend decoupled.
* Backend responsibilities:

  * [ ] **File management** – Local folder support (discouraged but implemented for completeness).
  * [ ] **Database backend** – Starting with embedded databases:
    * [ ] [DuckDB](https://duckdb.org/docs/stable/) / SQLite
    * [ ] Kuzu / [Ladybug](https://docs.ladybugdb.com/)
    * Later support for Postgres, Neo4j, Helix, Redis/Falkor, TypeDB, etc.
    * Later: Parsing tools like [treesitter](https://tree-sitter.github.io/tree-sitter/) (for code) or [cocoindex](https://cocoindex.io/docs/) (for documents)


* **Keep the frontend simple, fast, and extensible**. No heavy frameworks; Lumino manages layout.
* **Discourage very large files**; prefer small, structured artifacts.
* **Cross Platform Compatibility** Minus certain extensions it should run on Desktop, Mobile and Web

