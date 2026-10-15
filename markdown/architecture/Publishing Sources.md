---
title: "Publishing sources — reader mode, not a static site"
description: "Reader mode (the server read-only, so published sources stay queryable) and, as an idea for way later (2026-10-14), a light static reader that publishes a repository or a knowledge graph as wiki pages with code navigation, a graph and in-browser queries; Quartz stays until then. Also: why a plain static export would be pointless next to Quartz, and what is worth having instead - the existing web server in a read-only reader mode, so a published folder or database stays queryable (SQL, graph, search) by visitors; plus a plot panel later. Decision recorded 2026-09-20."
tags: [architecture, web, publishing, decision]
---
From [[Prompt18]] (2026-09-20), corrected the same day.

## The decision
A **static** Moonkale export (a Quartz-like site of pre-rendered files) is **dropped as a goal**. Daniel's argument: replacing Quartz is pointless unless the result does something Quartz cannot — being *queryable*. Think of someone going through proteomics/metabolomics sources and asking for a lot of data: a static site cannot answer SQL, cannot run a graph query over a database, cannot search anything that was not pre-scored. For text vaults Quartz is already fine and stays. So the value is entirely in the **live** half, and that half exists: the web server.

## What is worth having: reader mode
The web build already is "Moonkale on a website": the wasm client plus the server holding the sources under `MOONKALE_ROOT`, with token auth and the jail ([[Milestone 7 - Implementation Log]]). Publishing a source means **running that server read-only for visitors**:

- `moonkale serve --reader` (or `MOONKALE_READER=1`): every source is opened `read_only`; no terminal, LSP, agent, git operations, file ops or presence; no login required (or a token for a private audience); the same client in the reader-mode shell (no Save, no write rail entries — [[009]]).
- What visitors get that a static site never could: **SQL and graph queries** against the published SQLite/DuckDB/LadybugDB/Postgres sources (read-only, statement-classified, row/byte caps per request — the existing policy machinery), the **graph view** over the live index, **search** (BM25 + embeddings if configured), backlinks, the History panel over the entity log and git history, and — later — plots over any query result. A DuckDB data folder of CSV/Parquet is exactly the proteomics/metabolomics case: visitors query it, they do not download it.
- Deep links: `/doc/<key>`, `/node/<id>`, `/query?…` open the corresponding tab; per-visitor layout in `localStorage`.
- Costs and limits, stated: it is a process, not files — hosting, caps per visitor (rows, bytes, query time, concurrent connections), and rate limiting (present for `/login`, to be generalised). Crawlability: an optional per-document HTML fallback for search engines is the *only* piece of the static idea worth keeping, and it is small.

## Revisited 2026-10-14: a light static reader (way later)

> [!note] Status: an idea for much later, not scheduled
> Daniel, 2026-10-14: the **full web version stays** the web target. A **stripped-down web version** is wanted, but only **way later**. **Quartz stays** for publishing the vault until then. Nothing below is planned work. It records the shape so the idea isn't lost.

Daniel asked for a lightweight, wiki-like way to browse a GitHub repository or a knowledge graph on a website, read-only, "like Quartz but with some more features reasonable for this deployment". This would reopen one sentence of the 2026-09-20 decision above: "a static Moonkale export … is dropped as a goal". The rest of that decision (the full web client, reader mode as the server-backed way to publish) is untouched. The objection then was that a static site cannot answer queries. Two things have changed: the graph engine and the table grid are Rust/wasm components that run in a browser on their own, and small published data can be queried *in the browser*. So the proposal is **two tiers**, not one:

| | **Static reader** (new) | **Reader mode** (above) |
|---|---|---|
| Hosting | files: GitHub Pages, any CDN | a `moonkale-server` process |
| Built by | `moonkale publish <source> <out>`, run locally or in a GitHub Action | — |
| Pages | one pre-rendered HTML page per document, code file and graph node; works without JS, crawlable, fast | the full wasm client |
| Interactive parts | a small wasm reader client, loaded on demand | everything read-only |
| Queries | in the browser over published small files (SQLite and DuckDB/Parquet files, the index) | any source, any size, with visitor caps |
| Live, private or large data, graph-DB queries, agents | no | yes |

### What the static reader gives beyond Quartz

- **Code, not just notes.** Every source file is highlighted with the same tree-sitter grammars as the editor. Symbols become anchors, and go-to-definition, references and a per-symbol call graph come from the index built at publish time. A repository reads like a wiki of its own code.
- **One graph for notes and code**, in Moonkale's engine: the page's local graph in the sidebar, the whole graph on its own page, and the engine's layers (folders, kinds, languages) as toggles. The engine scales past where Quartz's graph view slows down.
- **Knowledge graphs as pages.** A graph source (a LadybugDB, HelixDB or TypeDB export) publishes one page per node: properties, typed edges grouped by kind, and neighbours. Backlinks work across notes, code and nodes.
- **Tables you can query.** A published SQLite or DuckDB file opens in the virtualised grid, with sort and filter, and a SQL box that runs in the browser. It needs a wasm SQL engine (a Rust one to evaluate first: Turso/limbo compiles to wasm; DuckDB's is JS-wrapped). Big files stay with reader mode.
- **Diagrams as diagrams:** `.flow.json` files render through the engine with their symbols. Typst, KaTeX and TikZ render to SVG at publish time.
- **History:** per-page "last changed", and recent commits and authors from git, without a server.
- **Search:** a BM25 index sharded at publish time, loaded on the first query.
- **Small extras:**
  - deep links to an anchor or a node;
  - "Open in Moonkale" (`moonkale://` or the web app) for readers who want to edit;
  - English, German and Chinese UI;
  - light and dark themes.

### Weight

The point is lightness.
- A page is HTML and one shared CSS file.
- The reader client is a **separate, small wasm build**, with no editors, terminal, agent, LSP or sources, loaded only when the reader opens the graph, a table, or search. The engine's wasm is about 2.7 MB today; the budget for the whole reader client is a few MB, split so the graph and SQL parts load separately.

### Steps, if it is ever scheduled

1. `moonkale publish` for a folder: Markdown and code pages, backlinks, the index as JSON, search shards, git metadata.
2. The reader wasm client: graph view (local and global) and search.
3. Code navigation from the index (definitions, references, call graph).
4. Graph-source pages and the table grid with in-browser SQL (after a wasm SQL engine is chosen).
5. A GitHub Action that publishes a repository to Pages. Whether it could one day replace Quartz for this vault is a question for then; until then the vault stays on Quartz.

## Plots and time series in a window (later)
A **Plot panel**: line / scatter / bar / histogram over a data frame from a query (SQL/DuckDB result, a CSV view, a `graph.query` result) or a live source (the Lenticulum viewer's beliefs over time — [[Julia and Lenticulum]]). Rendered in Rust (`plotters` → SVG/canvas in wasm; no JS charting library — [[JS Interop Boundary]]). In the app: open a table → *Plot…*; in reader mode the same panel over the visitor's query. Time axis, range selector, follow mode for live data.

## Steps (when scheduled)
1. Reader mode flag: read-only sources, capabilities stripped from `WorkspaceConfig`, reader-mode shell.
2. Visitor caps and rate limits on `/api/*` query endpoints; anonymous access option.
3. Deep links and per-visitor layout.
4. Optional HTML fallback per document for crawlers.
5. Plot panel.
