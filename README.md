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

**Milestone 4 — "Agents" — is done** (2026-09-18). An in-app agent
(Anthropic, OpenAI-compatible, Ollama, or an offline mock) that lists the
open sources, browses the graph, reads files, runs read-only SQL/Cypher and
searches — every tool call through a policy gate with approval prompts and
an audit log, transcripts saved as indexed pages; hybrid search (BM25 +
optional embeddings, Ctrl+Shift+F); stack traces and compiler errors drawn
as graphs from the terminal. That sits on Milestone 3's terminal, Typst
preview, LSP client and LadybugDB source; Milestone 2's index, wgpu graph
view, backlinks and SQLite tables; and Milestone 1's dockable workbench —
desktop and web from one codebase. Eleven automated browser suites cover it.

Still comment-only skeleton: Postgres/Falkor/KV drivers, Milkdown WYSIWYG,
WASM extensions, the flow editor. The **design vault** in `markdown/`
records every decision and is published at
**<https://mathstruct.github.io/Moonkale/>**; each implemented crate also has
a `<crate>.md` next to its code.

Next: Phase 5 of the roadmap — scale (GPU layouts), wasm extensions, the flow editor, mobile.

## Learn more

- Website / design docs: <https://mathstruct.github.io/Moonkale/>
- Start with *Overview*, then *Project Structure*, then *Roadmap*.
- Building, running, testing and how the site is published: the
  [Development](markdown/Development.md) page.

Moonkale is a [MathStruct](https://mathstruct.github.io/) project.
