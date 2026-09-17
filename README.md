# Moonkale (Under development)

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

**Milestone 1 — the walking skeleton — is done** (2026-09-17). You can open a
folder, browse it in a dockable workbench, open files in CodeMirror tabs,
edit, and save with conflict detection. On the web build the folder lives on
the server; on desktop it is read in-process. The same shell and editor code
runs on both, which is the point: the graph model, the JS interop boundary and
the extension API all exist and are exercised by an automated end-to-end test.

Everything else in the architecture — databases, indexing, the GPU graph view,
LSP, terminal, LLM tools, WASM extensions — is a documented crate skeleton
(22 crates under `packages/`, comment-only). The **design vault** in
`markdown/` records every decision and is published at
**<https://mathstruct.github.io/Moonkale/>**; each implemented crate also has
a `<crate>.md` next to its code.

Next: Phase 2 of the roadmap — the index (tree-sitter, wiki-links) and the
first graph view.

## Learn more

- Website / design docs: <https://mathstruct.github.io/Moonkale/>
- Start with *Overview*, then *Project Structure*, then *Roadmap*.
- Building, running, testing and how the site is published: the
  [Development](markdown/Development.md) page.

Moonkale is a [MathStruct](https://mathstruct.github.io/) project.
