---
title: "Publishing sources — reader mode, not a static site"
description: Why a static export of Moonkale would be pointless next to Quartz, and what is worth having instead - the existing web server in a read-only reader mode, so a published folder or database stays queryable (SQL, graph, search) by visitors; plus a plot panel later. Decision recorded 2026-09-20.
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

## Plots and time series in a window (later)
A **Plot panel**: line / scatter / bar / histogram over a data frame from a query (SQL/DuckDB result, a CSV view, a `graph.query` result) or a live source (the Lenticulum viewer's beliefs over time — [[Julia and Lenticulum]]). Rendered in Rust (`plotters` → SVG/canvas in wasm; no JS charting library — [[JS Interop Boundary]]). In the app: open a table → *Plot…*; in reader mode the same panel over the visitor's query. Time axis, range selector, follow mode for live data.

## Steps (when scheduled)
1. Reader mode flag: read-only sources, capabilities stripped from `WorkspaceConfig`, reader-mode shell.
2. Visitor caps and rate limits on `/api/*` query endpoints; anonymous access option.
3. Deep links and per-visitor layout.
4. Optional HTML fallback per document for crawlers.
5. Plot panel.
