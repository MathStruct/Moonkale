---
title: "Math — any delimiter, LaTeX or Typst, error only when both fail"
description: A request from 2026-10-14 (unnumbered). Every common math delimiter works ($, $$, \( \), \[ \], and their doubled-backslash forms); each formula renders as LaTeX (KaTeX) or Typst math, whichever it is; it errors only when neither parses. A command normalises a file or folder to one convention.
tags: [spec, markdown, math, typst, katex, planned]
---
**From Daniel (2026-10-14):** Markdown formulas sometimes come in `$`, sometimes `$$`, sometimes `\\(` or `\(` or `\[` or `\\[`, and all this is a pain. The Markdown editor should be robust against these choices, or be able to rewrite a file to one standard. Like the Wypst plugin, it should render in LaTeX formula style or Typst formula style, and only when both fail should it not render and show an error.

Status: **planned, not built.** It builds on [[013]] (KaTeX in the rich editor, done) and the Typst-math row of [[Markdown Diagrams and Math]], which had one setting per folder (`"math": "katex" | "typst"`). This request replaces that setting with a per-formula decision.

## 1. Delimiters: read them all

| Form | Display | Where it comes from |
|---|---|---|
| `$…$` | inline | Pandoc, Obsidian, Quartz (remark-math) |
| `$$…$$` | block (also inline when on one line inside text) | the same |
| `\(…\)` | inline | LaTeX, MathJax |
| `\[…\]` | block | LaTeX, MathJax |
| `\\(…\\)`, `\\[…\\]` | inline / block | Markdown that escaped its backslashes (LLM output, Jupyter exports, some converters) |
| ```` ```math ```` fence | block | GitHub |
| `` $`…`$ `` | inline | GitLab |
| a bare `\begin{equation}…\end{equation}` (also `align`, `gather`, …) | block | LaTeX pasted into notes |

**Rules, so prose and code stay prose and code:**
- Never inside code spans, code fences, HTML blocks, link destinations or front matter.
- **Dollar heuristics (Pandoc's):** an opening `$` must be followed by a non-space, a closing `$` must be preceded by a non-space and **not followed by a digit**. So "costs $5 and $10" is not math. `\$` is a literal dollar.
- A delimiter that opens and never closes within its block is text, not an error. Matching never spans paragraphs, except for `$$` and `\[`, which may span lines inside one block.
- The parser records **which delimiter it saw**, so the file is written back exactly as it was (no silent rewriting on save) and the normaliser (§3) knows what to change.

## 2. Renderer: LaTeX or Typst, per formula

Each formula is tried with **both** renderers: KaTeX for LaTeX, and the `typst` crate already compiled in for the Typst preview, which turns a formula into an SVG cached by content hash. The first that **parses and looks like its own language** renders it. The order is a setting (default: LaTeX first).

**Why "parses" is not enough.** Typst math is often valid LaTeX: KaTeX renders `integral_0^1 x^2 dif x` as italic letters without complaint. So each candidate gets a **confidence**, not only success or failure:
- **LaTeX signals:** `\commands`, `{…}` groups, `^{…}` and `_{…}`, environments.
- **Typst signals:** multi-letter Typst symbols without a backslash (`integral`, `sum`, `alpha`, `dif`, `arrow.r`, `RR`), quoted text `"…"`, `_(…)` and `^(…)` groupings, `frac(a, b)`-style calls, `.`-modifiers.
- A candidate that parses with no signal of its own language and with signals of the other loses to the other.
- **A tie** (`x^2 + y^2`: both parse, neither has signals) renders with the first renderer in the order. Both would look the same anyway.

**Only when both fail** does the formula not render. It then shows its source, outlined as an error, with both messages on hover (KaTeX's and Typst's), and it appears in the diagnostics of that document. It never breaks the rest of the document.

**Settings,** layered like every Moonkale setting (user → folder `.moonkale/markdown.json` → document front matter):

```json
{ "math": {
    "order": ["latex", "typst"],
    "force": null,
    "katex_macros": ".moonkale/katex.toml",
    "typst_preamble": ".moonkale/math.typ"
} }
```

- `order` sets the tie-break and the try order.
- `force: "latex"` or `force: "typst"` switches detection off for a folder or document, which is the old per-folder setting.
- Macros (spec [[013]]) apply to LaTeX; a Typst preamble (`#let` definitions) applies to Typst.
- A formula can also say what it is: `$$typst …$$`, or a fence ```` ```math typst ````, overrides detection.

## 3. Normalise: rewrite a file to one convention

A command, **Normalise math**, for one file, a selection, or a folder:
- **Target delimiters:** `$`/`$$` (the default, which Quartz and Obsidian read) or `\(`/`\[`.
- It also turns doubled backslashes (`\\(`) into single ones and bare `\begin{equation}` blocks into `$$` blocks.
- Optionally, it **marks each formula's language**: `$$typst …$$` for formulas detected as Typst, so another tool or another person's setting reads them the same.
- It always shows a diff first, touches only math (never code or prose), and is one undo step. A folder run lists the changed files and can be reverted as one entry in History.
- **On save:** a folder setting `"normalise_on_save": true` applies it automatically. It is off by default, since rewriting a file someone else also edits should be a choice.

Converting a formula *between* LaTeX and Typst (tex2typst-style) is a later, separate command. Normalising changes delimiters only.

## 4. Where it applies

- **Rich editor:** both renderers inline and in blocks; clicking a formula edits its source as today ([[013]]).
- **Source editor and the native live preview:** the same parser (one Rust crate, shared), so highlighting, folding and preview agree on what is math.
- **Index and search:** formulas are indexed as math (their source), not as prose.
- **The website (Quartz):** remark-math reads `$` and `$$` only, so the normaliser's default target is what the site needs. A note written with `\(` renders in Moonkale today and on the site after normalising.

## 5. Acceptance

- A fixture note containing every form in the table renders every formula.
- "costs $5 and $10" stays text, and code spans and fences are untouched.
- Side by side: `\int_0^1 x^2\,dx` renders with KaTeX, `integral_0^1 x^2 dif x` with Typst, `x^2+y^2` with the first in `order`, and `\frac{a}{` shows the error with both messages.
- Normalising the fixture yields only `$`/`$$`, and re-reading it gives the same formulas, rendered the same.
- Saving a file without normalising leaves its bytes unchanged.

Work package: WP-36 in [[Work Packages]].
