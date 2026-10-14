---
title: "Editor prior art — Obsidian, VS Code, Zed: what to take, what holds us back"
description: The features people expect from Obsidian, VS Code and Zed, each judged for Moonkale as taken, adapted, or rejected because it would limit Moonkale. Requirements from public documentation only. Study of 2026-10-14.
tags: [research, prior-art, requirements, scrutiny, obsidian, vscode, zed]
---
**Daniel (2026-10-14):** fine to take the specs and requirements of Obsidian, VS Code and Zed, but not where they hold Moonkale back. So every item below has a verdict:
- **Taken** (✅ means already in Moonkale);
- **Adapted:** taken, with the change named;
- **Rejected:** with the reason, and an entry in [[Scrutiny]].

Requirements come from public documentation and behaviour only ([[Scrutiny]] rules). VS Code (MIT, but the product adds proprietary parts) and Zed (GPL and AGPL for most crates, Apache for GPUI) are read for behaviour, never for code. Obsidian is proprietary, but its **JSON Canvas** file format is an open MIT spec and can be implemented.

## What each one is built around, and why that matters

The three are built on assumptions Moonkale doesn't share. Most rejections below come from these:
- **Obsidian: "a vault is a folder of Markdown files".** The files are the only truth, and everything else (the graph, search, metadata) is derived from them.
- **VS Code: "everything is a text document in a workspace folder".** Its extension API, its views and its language features are organised around `TextDocument`s and URIs.
- **Zed: "a fast native text editor".** It is a GPU-rendered UI, deliberately narrow, and its extensions can't add arbitrary UI.

**Moonkale's own assumption is "everything is a node in a graph, from any source"** ([[Graph-Native Model]], [[North Star]]). Files are one source among databases and graph stores, and text is one projection.

## Notes (Obsidian)

| Feature | Verdict |
|---|---|
| Wiki-links, backlinks, unresolved links, rename rewrites | ✅ (spec 012) |
| Embeds `![[note]]`, block references `^id`, heading links | **Taken**, open: embeds and hover previews are deferred in spec 012. Block ids become node ids |
| Properties (front matter) with types | ✅ (spec 019). **Adapted:** properties are node properties, so the same view works on a database row |
| Callouts, highlights, comments `%%` | **Taken** (rich editor), the parser shared with the source editor |
| Graph view, local graph | ✅. **Adapted:** in Moonkale the graph is primary, not a decoration |
| Canvas | **Taken, with the open JSON Canvas format** (read and write `.canvas`), drawn on the graph engine with the planned canvas components (WP-05) |
| Bases (table views over notes' properties) | **Adapted:** a query over node properties, shown in the table grid, which works for any source, not only notes |
| Daily notes, templates, command palette, quick switcher | ✅ palette and quick open; daily notes and templates **taken** (small; a template is a document plus front-matter defaults) |
| Plugins: JS with full access to the app and the DOM | **Rejected:** no permissions, no isolation. Moonkale's extensions are Rust or sandboxed wasm with permissions and tiers ([[Extension System]]) |
| Sync and Publish as paid hosted services | **Rejected as services.** Moonkale: the state store and [[Collaboration]] for sync; reader mode and Quartz for publishing ([[Publishing Sources]]) |
| "Files are the only truth" | **Rejected** (the assumption above). Files stay a first-class source, and notes on disk stay plain Markdown that Obsidian opens |

## Code (VS Code)

| Feature | Verdict |
|---|---|
| Command palette, quick open, keybindings with `when` clauses | ✅ palette, quick open, keybindings; `when` clauses **taken** (designed, [[Contribution Points]]) |
| LSP integration | ✅ |
| Debug Adapter Protocol | **Taken** ([[Debugger Requirements]]) |
| Tasks (`tasks.json`), problem matchers | **Adapted:** tasks are runs with run records (shared with workflows and Lenticulum, WP-38/41); problem matchers become diagnostics from a run's output |
| Testing API (test explorer, run and debug tests, coverage) | **Taken** later; KG Studio and Lenticulum both have tests to show |
| SCM API | ✅ Git; **adapted:** SCM history is a graph |
| Remote (SSH, containers, WSL), a headless server | ✅ SSH and the server; dev containers **taken** later |
| Settings: JSON with schemas, UI editor, user and workspace scopes, profiles | ✅ layered, typed settings; profiles **taken** (they also answer "on by default per kind of user", [[Core and Extensions]]) |
| Notebooks API | **Taken** ([[Notebook Editor]]) |
| Extension host in Node with an untyped JS API, and webviews as iframes | **Rejected:** a second runtime and a second UI model. Moonkale extensions contribute Dioxus components and services directly |
| Every view is about a `TextDocument` | **Rejected** (the assumption above). Moonkale views take nodes |
| Live Share (proprietary service), Settings Sync through an account | **Rejected as services.** Moonkale: CRDT collaboration and the state store ([[Collaboration]]) |
| Copilot or chat locked to one vendor | **Rejected:** Moonkale's agent panel takes any provider, under the policy gate |
| Accessibility (screen readers, keyboard-only use, high contrast) | **Taken as a requirement:** VS Code is the bar. Moonkale has keyboard navigation in the graph (spec 031 stage 10) but no audit yet |

## Speed and structure (Zed)

| Feature | Verdict |
|---|---|
| GPU-rendered UI aiming for 120 FPS | **Taken as a bar, adapted to 60 FPS** for the code editor (CED-R22). Moonkale renders through a webview (Dioxus), and the graph through wgpu |
| **Multibuffers:** one editable buffer of excerpts from many files (search results, references, diagnostics) | **Taken, and generalised:** a buffer of *nodes*, as excerpts from files, database rows, graph nodes and definitions in a code graph, edited in place. This is a projection in the [[North Star]]'s sense, and a strong fit |
| Extensions as wasm components (WIT) | **Taken as the direction:** Moonkale's wasm ABI is JSON v1 today, components later ([[ADR-0013 JSON ABI before components]]) |
| Extensions can't add arbitrary UI | **Rejected:** that restriction would forbid the flow editor, the table and every Moonkale panel. Moonkale gates power with permissions, not with less UI |
| Tree-sitter for highlighting and structure, outline panel | ✅ |
| Remote development with a headless server | ✅ |
| Collaboration channels (CRDT buffers, shared projects, calls) | **Taken** (CRDT editing) without the calls ([[Collaboration]]) |
| Edit predictions, an assistant panel with context | **Adapted:** the agent panel with the graph as context (the agent can query the index and the sources) |
| Vim mode | **Taken** later, as a keymap extension |
| A worktree of files as the only project model | **Rejected** (as Obsidian's) |

## What this adds to the work

- **Multibuffers over nodes:** a strong match for the North Star. Proposed as WP-43.
- **JSON Canvas read and write** on the graph engine: WP-44, after the canvas components (WP-05).
- **Accessibility audit:** added to WP-26 (upkeep).
- Embeds, hover previews, daily notes, templates, `when` clauses, profiles, a test view and dev containers: small, and listed for the specs list when Daniel wants them.
