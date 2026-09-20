---
title: "Milestone 10 — Daily use: the specifications"
description: Running log of Milestone 10 — Moonkale used as the daily editor, with the small specifications in markdown/specifications/ as the backlog; each entry names the spec, what was built and the suite that covers it.
tags: [milestone, log]
---
After Milestone 9 the work changed shape: Daniel uses Moonkale daily and files [[README|specifications]] (one `NNN.md` per request or bug); each is implemented, verified with the native tests and a browser suite, and the spec file is edited in place with a "Done" paragraph. This page is the milestone's log; the bigger designs waiting behind it are listed under *Phase 10 candidates* in [[Roadmap]].

## Done

| date | spec | what was built | verified |
|---|---|---|---|
| 2026-09-20 | [[013]] Formulas | KaTeX in the rich editor: Crepe's Latex feature on, KaTeX css + woff2 fonts as a dx **folder asset** (`asset!("/assets/katex", AssetOptions::folder())`) linked through the new `StylesheetUrl`; per-folder macros from `.moonkale/katex.json` | `rich.mjs` (render, css, fonts, macro, disk unchanged); desktop `dx build` bundles `assets/katex/` |
| 2026-09-20 | [[012]] Wiki-links | `ext-api::wiki` (parse, resolve, complete, follow-or-create, backlinks, rename rewrite) + a ProseMirror decoration plugin (`js/milkdown/src/wiki.ts`: hidden brackets, plain click, `[[` popup) + CodeMirror marks/completion/Ctrl+click for markdown sources + **Create** in the Links panel + `rename_node` rewriting links in every linking file | `wiki.mjs` (8 steps), 2 unit tests; `rich.mjs` adapted |
| 2026-09-20 | [[010]] Highlighting | grammars in the CodeMirror bundle (decision P-093): 12 Lezer + 5 legacy modes, folding, bracket matching, `Mod-/`; `language_hint` extended to the whole core list; LSP discovery for clangd, nil/nixd, taplo, tinymist, LanguageServer.jl, JSON/YAML/GraphQL servers | `highlight.mjs`; `lsp.mjs`/`lsp2.mjs` made token-aware |

## Numbers
- Browser suites: 29 (+`wiki`, `highlight`), all PASS in one batch (2026-09-20); native tests 84 pass / 0 fail / 3 ignored; clippy and fmt clean; desktop builds with dx.
- Bundles: `codemirror.js` 368 KB → 1 021 KB (grammars); `milkdown.js` 2.69 → 2.75 MB (KaTeX) + `assets/katex/` 22 KB css + 20 fonts.

## Problems hit (→ [[Problem Log]])
- **P-093** highlighting grammars in JS (decision).
- Forgetting to rebuild a JS bundle after editing its TypeScript cost one debugging round (`moonkale-wiki` plugin key absent from the served file) — `npm run check` is not `npm run build`.
- Test selectors: `:visible` is Playwright-only and invalid inside `page.evaluate`; hidden bracket spans share the link class, so tests select `.mk-wikilink:not(.mk-wiki-bracket)`; with highlighting a CodeMirror line is many text nodes, so XPath lookups walk the line's text nodes.

## Open (from the specifications folder)
[[006]] Android pinch/rotate, [[007]] Android app name + icon, [[008]] image viewer, [[009]] activity bar/menus/source icons, [[011]] closeable panels; deferred parts of 012 (hover preview, embeds).
