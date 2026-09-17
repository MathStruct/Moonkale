---
title: "index — implementation notes"
tags: [crate-notes, milestone-2]
---
Notes for `moonkale-index` (Milestone 2). Design: [[Indexing]].

- `graph.rs` — `IndexGraph`: mirrored folder nodes + derived nodes; edges keyed by the file they came from (`set_derived(origin, nodes, edges)` replaces a file's contribution atomically); `by_stem`/`by_path` maps for link resolution; `neighbours` (BFS with direction), `all` (kind filter, cap, files first), `children`.
- `extract/wikilinks.rs` — `[[Name]]`, `[[Name#h|alias]]`, `[[dir/Name]]`, `[text](rel.md)`; Obsidian resolution (stem, case-insensitive, shortest path wins); unresolved targets become phantom `Page` nodes owned by the index.
- `extract/symbols_rust.rs` — tree-sitter: fn/struct/enum/trait/mod/impl/const/static/type/union/macro, nested items via `body`, `Defines` from the file and `Contains` from the parent item. Keys include the line number.
- `walk.rs` — BFS over `Query::Children`, skips `.git`/`target`/`node_modules`, caps files and text size.
- `source.rs` — `IndexSource::build(folder)`; id `index:<folder id>`; root = the folder's root; `fetch_text` delegates to the folder for mirrored nodes; `refresh(node)` re-extracts one file; `descriptor().display_name` carries the stats.
- Native only (tree-sitter's C runtime); an empty crate on wasm32.

Tests: `cargo test -p moonkale-index` (4 integration on a tempdir vault + 2 unit).
