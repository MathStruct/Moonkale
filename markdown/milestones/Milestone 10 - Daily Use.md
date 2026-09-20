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
| 2026-09-20 | [[017]] Graph reset | incremental `set_graph` in the renderer: shared nodes keep positions/pins, new nodes placed by neighbours, warm relayout only when the set changed, camera untouched | `graph3d.mjs` step |
| 2026-09-20 | [[016]] "Loading editor…" hang | code editor mounts from `onmounted` (P-047 family), `try/catch` around `cm.mount` → `BackendEvent::Failed` shown in the panel, grammars degrade to plain text on error | editor suites; the desktop case is timing-dependent — Daniel's next open of the file |
| 2026-09-20 | [[006]] Android gestures | `Drag::Pinch` in the renderer: touch pointers with capture, pinch = zoom at the midpoint, midpoint = pan, rotation = yaw in 3D; `camera_state()` export | `graph3d.mjs` step (CDP touch), `android-pinch.mjs` on the phone |
| 2026-09-20 | [[007]] Android name + icon | `packages/mobile/build-android.sh`: dx build → `strings.xml` + generated launcher/adaptive icons → Gradle reassemble → install | drawer shows Moonkale with the icon |
| 2026-09-20 | [[009]] Activity bar, menus, source icons | `Activity` on `PanelContribution` (one registry for rail + phone bar, badges, hide-on-click), Ctrl+B/Ctrl+J, Save All / Close All, `EditorAction` commands routed to CodeMirror (`run`), menus built from the registry (Show ▸, extension menus by category), `ui::icons` (SVG icons, `source_icon`, `source_color`), Explorer source rows with icon/stripe/lock, status-bar colour dot | `shell.mjs`, `phone.mjs` extended |
| 2026-09-20 | [[008]] Image viewer | new `editors/image` extension (optional): blobs open as views, `Source::fetch_bytes` (+ server fn, remote), data-URL `<img>` with fit/zoom/pan, SVG **Source**; Explorer opens image blobs | `image.mjs`, 1 unit test |
| 2026-09-20 | [[011]] Closeable panels | `closable: true` everywhere; `Workspace::closed_panels` + `show_panel`; `View: Show <panel>` generated for every static panel; pruned tiles come back at their edge with the default ratio (`home_zone`) | `panels.mjs` |
| 2026-09-20 | [[015]] Close folder | `Workspace::close_source` (documents + derived index, refuses while unsaved, flushes history, leaves presence), `Command::CloseFolder` / `workspace.closeFolder`, File menu, Explorer root context menu; `reopen_last` user setting | `files.mjs` step |
| 2026-09-20 | [[014]] Word wrap | `editor.wrap` user setting; toolbar **Wrap**, **Alt+Z** (`editor.toggleWrap`), Settings → Editor; live in every open editor via a CodeMirror compartment | `highlight.mjs` step |
| 2026-09-20 | search hardening (no spec: "the search function isn't working") | HTTP providers get a 10 s connect timeout and embeddings a 20 s request timeout; the query embedding in `IndexSource::run_search` has a 3 s budget and falls back to keyword-only — a stalled embedding call no longer hangs a search | native tests; desktop build |
| 2026-09-20 | [[010]] Highlighting | grammars in the CodeMirror bundle (decision P-093): 12 Lezer + 5 legacy modes, folding, bracket matching, `Mod-/`; `language_hint` extended to the whole core list; LSP discovery for clangd, nil/nixd, taplo, tinymist, LanguageServer.jl, JSON/YAML/GraphQL servers | `highlight.mjs`; `lsp.mjs`/`lsp2.mjs` made token-aware |

## Numbers
- Browser suites: 33 (+`wiki`, `highlight`, `panels`, `image`, `shell`), all PASS in one batch (2026-09-20, second run of the day: 31 suites in the batch + `milestone1` + `auth` on its own server); native tests 85 pass / 0 fail / 3 ignored; clippy and fmt clean; desktop builds with dx; `mobile` checks.
- Bundles: `codemirror.js` 368 KB → 1 021 KB (grammars); `milkdown.js` 2.69 → 2.75 MB (KaTeX) + `assets/katex/` 22 KB css + 20 fonts.

## Problems hit (→ [[Problem Log]])
- **P-093** highlighting grammars in JS (decision).
- Forgetting to rebuild a JS bundle after editing its TypeScript cost one debugging round (`moonkale-wiki` plugin key absent from the served file) — `npm run check` is not `npm run build`.
- **P-094** `load` events on `<img>` never reach dioxus's delegated listener (non-bubbling); use `onmounted` + `img.decode()`. Also: an eval script is a *function body* — a bare IIFE expression returns `undefined`; `return` the value.
- Test selectors: `:visible` is Playwright-only and invalid inside `page.evaluate`; hidden bracket spans share the link class, so tests select `.mk-wikilink:not(.mk-wiki-bracket)`; with highlighting a CodeMirror line is many text nodes, so XPath lookups walk the line's text nodes.

## Open (from the specifications folder)
(none from the numbered specs — 001–015 are done, 002/003/004/005 documented); deferred parts of 012 (hover preview, embeds). Open question: whether the search stall was the embedding call (fixed) or something else Daniel sees.
