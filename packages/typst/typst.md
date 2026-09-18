---
title: "typst — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-typst` (Milestone 3). Native only (the web build compiles on the server through `api::compile_typst`).

- `MoonkaleWorld` implements `typst::World` over a folder root: the main file's text comes from the editor (unsaved edits preview live); other files (`#include`, images) are read from disk under the root; fonts come from `typst-assets` (embedded, so no system fonts are needed); `Library::default()`; `today()` returns `None`; packages (`@preview/...`) are **not** supported yet (no download).
- `compile_to_svg(root, main_rel, text) → Result<Vec<String>, Vec<Diagnostic>>` — one SVG string per page via `typst-svg`; errors carry message + hint.
- Versions: `typst` 0.15, `typst-layout` (`PagedDocument`), `typst-svg`, `typst-assets` (fonts).

Tests: `cargo test -p moonkale-typst` (a page compiles; a syntax error surfaces as a diagnostic).
