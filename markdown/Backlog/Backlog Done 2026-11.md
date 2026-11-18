---
title: "Backlog Done 2026-11"
description: Work packages (and finished slices of open packages) of November 2026 — date, commit, what was done, and how it was verified.
tags: [planning, backlog]
---
Finished work of November 2026, newest first. A slice of a package whose row stays open in [[Work Packages]] is listed here too, with what remains. Convention: [[Backlog]].

| WP | Done | Commits | What | Verified |
|---|---|---|---|---|
| 40 (PLT-09) | 2026-11-18 | `feat(plot): live tables and plots …` (dev-claude) | Live plots for file-backed SQL sources: SQLite/DuckDB/Turso `changes_since` by file fingerprint, `SourcesState::data_epochs`, tables re-run (pushed-down grids in place), plots and source aggregates redraw in place. Row stays open: scrub cursor (LT-12, needs WP-38), PDF/PNG export (Daniel) | `sql` tests `changes` and `stamp`, `ext-api` test `follow` (pacing, no index refresh), browser `plot-live.mjs` (CSV append, SQLite insert, zoom kept, aggregate not partial) plus plot, table-edit, table-grid, duckdb, watch, links-sqlite |
| 40 (step 2) | 2026-11-18 | 51e83519 | Source aggregation (PLT-11), `.plot.json` specs (PLT-04), SVG export | see [[Status]] and [[P-243 Plot step 2 review findings]] |
