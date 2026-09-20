---
title: "Markdown — Typst math, TikZ, Mermaid, tabs"
description: What Daniel uses on top of plain markdown in Obsidian (Wypst for Typst formulas, TikZJax for TikZ diagrams, Mermaid, tabs) and how each comes to Moonkale — Rust where an engine exists (Typst is already compiled in), JavaScript as a desktop-only extension where it does not (TikZ, Mermaid).
tags: [editors, markdown, design]
---
From [[Prompt19]] (2026-09-20). Two Obsidian plugins and two conventions Daniel relies on, plus the rule he set: JavaScript for these is permitted **as an extension for the desktop editor** when there is no Rust engine — the one sanctioned exception beyond the three editor bundles ([[JavaScript Inventory]]). Formulas via KaTeX already work ([[013]]).

| feature | in Obsidian | markdown syntax | how in Moonkale | engine |
|---|---|---|---|---|
| **Typst math** | *Wypst* renders `$…$` / `$$…$$` with Typst instead of MathJax/KaTeX | the same `$…$` — the *content* is Typst math (`integral_0^1 x^2 dif x = 1/3`) rather than LaTeX | a setting per folder, `.moonkale/markdown.json` → `"math": "katex" \| "typst"` (default katex): with `typst`, the rich editor's math nodes are rendered by **the `typst` crate that is already compiled in** for the Typst preview — each formula becomes an SVG (`typst` compiles `$…$` inside a minimal page, `typst-svg` renders), cached by content hash; Quartz already renders Typst math for the site via `@myriaddreamin/rehype-typst`, so the vault reads the same both places | **Rust**, exists; the wiring is a Milkdown node view that asks Rust for an SVG instead of calling KaTeX (`onMath(id, source) → mathResult(id, svg)`) |
| **TikZ diagrams** | *Inline TikZ / TikZJax* renders ```` ```tikz ```` fences with TeX compiled to wasm in the page | ```` ```tikz ```` code fence (a full `\begin{tikzpicture}…` body; optional `\usepackage` lines) | a **desktop-only extension** `tikz` (opt-in) with two backends behind one trait, chosen by what is installed: (1) **native TeX** — `tectonic` or `pdflatex` + `dvisvgm`/`pdftocairo` on `PATH`, `tikz` fence → SVG, cached; (2) **TikZJax** — the wasm TeX engine + its JS glue as a `packages/js/tikzjax` bundle (≈ 10 MB with the TeX memory dump; loaded on first use, never in the web build). Rust rewrite: none exists — a TeX engine in Rust is not on anyone's roadmap; **CeTZ** (Typst's TikZ analogue) is the Typst-native way to draw, and a ```` ```cetz ```` fence rendered by the compiled-in `typst` crate is the Rust path for *new* diagrams, offered next to `tikz` | **JS or an external binary**, desktop only; never on web or mobile |
| **Mermaid** | built in (also in Quartz: this vault uses it) | ```` ```mermaid ```` fence | a `packages/js/mermaid` bundle behind the same code-fence renderer trait (`FenceRenderer { languages, render(source) → svg/html }`), loaded lazily; no Rust Mermaid exists (the diagram *layout* engines are the hard part). Platform: desktop and web (it is plain JS, 2.5 MB, so lazy and opt-in on the phone) | **JS** |
| **Tabs** | an Obsidian plugin renders ```` ```tabs ```` blocks (`tab: Title` sections) as a tab strip | ```` ```tabs ```` fence with `tab:` headers | pure Rust/Dioxus: the fence renderer trait's first native implementation — a tab strip whose panes are markdown rendered by the rich editor itself; source stays a fence so Quartz/GitHub degrade to a code block (a Quartz plugin later) | **Rust** |

## The one mechanism: code-fence renderers
The rich editor gets a **fence renderer contribution** (`FenceRenderer` in `ext-api`, contributed by extensions): a fence whose language matches is shown as the renderer's output (SVG/HTML/Dioxus) with *click to edit* back to the source, exactly like the KaTeX block today. Tabs (Rust), CeTZ and Typst math (Rust via `typst`), Mermaid and TikZJax (JS bundles, desktop/web as stated) all plug into it; the source editor shows the fence with highlighting. Rendering is asynchronous and cached by content hash under `.moonkale/cache/`.

## Platform and rules
- **Rust first where an engine exists**: Typst math, CeTZ and tabs never touch JavaScript.
- **JS only as an opt-in desktop extension** (Daniel's exception): TikZJax; Mermaid is allowed on web too because it is just JS and Quartz users expect it.
- **Never in the core bundle**: each is its own lazily loaded asset; the phone gets tabs and Typst math, not TikZ.
- Static/Quartz parity: the markdown stays valid for Quartz (`rehype-typst`, Mermaid, code fences); tabs need a small Quartz transformer to look the same on the site.

## Order (when scheduled)
1. Fence renderer contribution + **tabs** (proves the mechanism in pure Rust).
2. **Typst math** setting, SVG through the `typst` crate; CeTZ fence with the same path.
3. **Mermaid** bundle (JS, lazy).
4. **TikZ** extension: native TeX backend first, TikZJax second.
