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

## Milestone 4: search
`search.rs` — `SearchIndex`: text files are split into ~40-line chunks (cut at blank lines), tokenised (lowercase words; `snake_case`/`camelCase` parts as well as the whole identifier), ranked with **BM25**; when chunks carry embeddings and a query vector is available, a cosine ranking is fused in by reciprocal-rank fusion. `IndexSource::build_with(folder, embedder)` accepts a `moonkale_llm::Provider`; `embed_pending()` fills vectors in batches (desktop and server call it in the background after open, so search is BM25-only until it finishes) and `refresh` re-chunks one file. Exposed as `Query::Text { dialect: "search", text }` → a table (`path, line, score, snippet`) plus the matching `File` nodes — so `RemoteSource` carries it to the web client unchanged. Used by the Search panel (`ui/src/search.rs`, Ctrl+Shift+F) and the `index.search` tool.

## Milestone 7
`refresh(node)` handles the three new cases: a vanished node (rename/delete) drops its subtree, derived data and `Contains` edges and re-extracts the files that linked to it (their links become phantoms); a new directory is walked; a new file gets its parent's `Contains` edge and the files whose phantom links it resolves are re-extracted. `IndexGraph::{subtree, remove_subtree, origins_linking_to, phantoms_for, add_edge}`. Test `refresh_follows_rename_delete_and_new_directories`.
