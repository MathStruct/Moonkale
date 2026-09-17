# Moonkale

**A graph-native code and knowledge editor.**

Moonkale opens *folders and databases* — a source tree, a Postgres schema, a
TypeDB or LadybugDB graph, a Redis keyspace, an Obsidian-style wiki — and shows
them as **one graph**. Every editor is a view on that graph: a code editor with
language-server support, a WYSIWYG Markdown/Typst editor with wiki-links, a
table/SQL editor, a GPU-rendered 2D/3D graph view that stays fluid at 100k+
nodes, a drag-and-drop flow editor (first target: building Lux.jl models), and
a terminal whose stack traces become clickable subgraphs. Language servers,
indexers and LLM agents all work on the same graph through the same doors.

It is built in **Rust** with [Dioxus](https://dioxuslabs.com) for desktop, web
and mobile from a single codebase, and it is **extension-driven**: the built-in
editors are extensions with no privileged access, so anything they can do, a
third-party extension can do.

Moonkale grew out of the frustration that code editors, knowledge editors,
database tools and no-code tools are separate worlds — and that none of them
were built with graph databases or retrieval-augmented agents in mind.

## Where it stands

**Early design phase.** What exists today:

- a dockable workbench prototype (tabs, splits, drag-to-dock, activity rail,
  status bar) running on desktop and web, with placeholder panels;
- a complete **crate skeleton** for the architecture — 22 crates under
  `packages/`, each documented in comments describing what it will hold and
  why, compiling as empty modules;
- the **design vault** in `markdown/`: architecture, per-editor designs, an
  extension-authoring guide, eleven decision records, a ranked problem list
  and a phased roadmap. It is published at
  **<https://mathstruct.github.io/Moonkale/>**.

Nothing beyond the workbench shell is implemented yet. The next step is Phase 1
of the roadmap: the core graph model, a folder source, and the first code
editor.

## Learn more

- Website / design docs: <https://mathstruct.github.io/Moonkale/>
- Start with *Overview*, then *Project Structure*, then *Roadmap*.
- Building, running, testing and how the site is published: the
  [Development](markdown/Development.md) page.

Moonkale is a [MathStruct](https://mathstruct.github.io/) project.
