---
title: "Markdown and Typst Editor"
tags: [editor, markdown, typst]
---
Crate: `editors/markdown`. The knowledge half of the project.

## Markdown (Obsidian conventions)
- `[[wiki-links]]`, `[[page#heading]]`, `[[page|alias]]`, `![[embeds]]`, frontmatter, tags, callouts.
- **WYSIWYG** via Milkdown (ProseMirror) behind `RichTextBackend` ([[JS Interop Boundary]]); source mode = the [[Code Editor]] with `lang: markdown`.
- Milkdown round-trips to Markdown; we store Markdown, so ProseMirror-only state is intentionally lost. That's what makes the backend replaceable.
- **Rust-side model** (`pulldown-cmark` → block tree) exists regardless of backend: outline, link extraction, embeds and rename-propagation work without JS.

**Parser fallback (P-212, 2026-10-17):** the native Markdown presentation,
math/list/table/callout/image providers, wiki preview and documentation rendering
retain plain source when parsing fails. NUL, vertical tab and form feed bypass
pulldown-cmark before parsing on every profile; unwind builds also catch complete
parse failures. The known 0.13.4 offset-iterator crash is covered by stable tests
and four fuzz corpus inputs. Canonical bytes are unchanged; see
[[P-212 Markdown offset iterator can panic]] and [[Security]].

## Links are edges
Every link is a `Links` edge extracted by [[Indexing]]. Backlinks = `Query::Neighbours{direction: In, kind: Links}`. The "local graph" pane is the [[Graph View]] at depth 1 — no separate implementation. Links can target **any node**, including a database row or a symbol: `[[users]]` may resolve to a Postgres table. Resolution uses Obsidian's shortest-unique-path rules, extended with source scoping.

## Typst
- `typst` is pure Rust and compiles to wasm → **in-process compile on every platform** ([[ADR-0007 Typst via the native crate]]).
- `World` impl backed by the folder source (files, fonts, packages cache).
- Editing: code editor + live SVG preview. A structured/WYSIWYG Typst editor is research (Typst's AST is available via `typst-syntax`, which makes a block-level editor plausible later).
- Diagnostics map to editor ranges like LSP diagnostics.

## Diagrams
Mermaid blocks render in preview via a small JS package if/when needed (same interop rules); Typst diagrams (CeTZ) come for free with Typst. Excalidraw: open question — embed the web app behind the interop boundary, or treat `.excalidraw` JSON as a `Block` graph in the [[Flow Editor]] canvas. Leaning to the latter, later.

## Phases
1. Source-mode markdown with link completion + backlinks + local graph.
2. Milkdown WYSIWYG.
3. Typst preview.
4. Embeds of *any node* (`![[users]]` renders a table).

## Relative images in Rich mode (P-218, 2026-10-22)

Rich image node views resolve Markdown destinations relative to the document's
folder in its source, through the same source lookup, image limits, freshness
checks and bounded data-URL cache as native Markdown previews. The view keeps
the original destination in Markdown, including after editing and saving.
Missing, escaping and external destinations receive no display URL. Each mount
requests at most 128 images with destinations no longer than 4096 bytes.
`rich-images` covers a percent-encoded subfolder image, refusal of outside and
remote paths, and saving without embedding display URLs.
