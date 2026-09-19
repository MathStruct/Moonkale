---
title: "Unicode input — \\name → symbol, everywhere"
description: Typing `\int` and getting ∫, `\alpha` → α, `\:ladybug:` → 🐞 with a dropdown while typing — the Julia/Lean way — as a default of every text input in Moonkale, from an editable, extensible table with several names per symbol. Design and plan, not implemented.
tags: [editors, input, design]
---
From [[Prompt11]] (2026-09-19). **Desired behaviour**: in any place Moonkale accepts text — code editor, markdown source and rich editor, table cells, flow parameter fields, agent chat, search and palette inputs, terminal — an escape character (default `\`) followed by a name offers the matching symbols in a dropdown while typing and replaces the sequence with the character on Tab/Enter, exactly as the Julia REPL, Julia's and Lean's VS Code extensions do. Like pinyin input, but done by Moonkale, not the OS (OS input methods keep working underneath; we never intercept them).

## Behaviour
- **Trigger**: `\` at a word boundary (start of line, after whitespace or punctuation — not inside `\\` string escapes in code, not after a backslash that is itself escaped). Configurable globally (`input.unicode.escape`, default `\`) and **per language** (`input.unicode.escape_by_language`): LaTeX, Markdown and notebooks default to `;` because `\` is their source syntax (below); Typst keeps `\`.
- **While typing** `\al…` a dropdown lists matches ranked: exact name first, then prefix, then fuzzy over *all* names of a symbol (`\integral` and `\int` both reach ∫; the dropdown shows `∫  \int · \integral`). Each row: symbol, names, a short description (Unicode name), and where it came from (Julia table, Lean table, user file, extension) in the detail column.
- **Accept**: Tab or Enter replaces `\name` with the symbol; Escape keeps the literal text; typing a space or any non-name character with a **unique exact match** replaces eagerly, Lean-style (`input.unicode.eager`, default on — Julia's REPL is Tab-only and that is one setting away).
- **Emoji**: `\:name:` (Julia's convention) — the closing colon is part of the name so `\:` alone lists nothing until a letter is typed; `\:ladybug:` → 🐞.
- **Cursor placeholders**: names may expand to a template with a caret position — Lean's `\<>` → `⟨|⟩`, `\[[]]` → `⟦|⟧`; written `⟨$CURSOR⟩` in the table.
- **Composition**: `\alpha\hat` → α̂ works by expanding one at a time (combining characters are just symbols; Julia's `\hat` is U+0302). Nothing special.
- **Undo** restores the typed name, not the character, in one step.
- **Reverse lookup**: hovering a non-ASCII character shows its names (`∫ — \int, \integral`), and the palette command *Insert symbol…* (`edit.unicode.insert`) offers the same dropdown for inputs that cannot host it (e.g. a native file dialog cannot, the terminal can via the palette).

## The table
Three layers, merged, later ones override or add names:

| layer | file | contents |
|---|---|---|
| **built-in** | `packages/ui/assets/unicode/{latex,emoji,lean}.toml` | Julia's `latex_symbols.jl` (≈ 1 500 names, MIT, generated from the W3C `unicode.xml` mapping) and `emoji_symbols.jl` (≈ 1 250), and Lean's `abbreviations.json` (≈ 1 850, Apache-2.0, with `$CURSOR` templates). Converted at build time by a script in `packages/ui/scripts/`, with the licences kept next to the files. Overlaps (both have `\alpha`) are one entry with two provenance tags. |
| **user** | `~/.config/moonkale/unicode.toml` (`MOONKALE_CONFIG_DIR` on the server; `files/` on the phone) | additions and overrides; edited in *Settings → Input* with a small table editor (name, symbol, description) or by hand |
| **workspace** | `<folder>/.moonkale/unicode.toml` | project-specific names (a paper's notation, a codebase's operators), committed with the folder |
| **extensions** | `unicode` contribution in the manifest ([[Contribution Points]]): a TOML file in the package, optionally scoped to languages (`languages = ["lean"]`) | a Lean extension brings Lean's table; a chemistry extension brings its own |

File format (one symbol, many names; the first name is the canonical one shown in reverse lookup):
```toml
version = 1

[[symbol]]
char = "∫"
names = ["int", "integral"]
description = "INTEGRAL"          # optional; defaults to the Unicode name

[[symbol]]
char = "🐞"
names = [":ladybug:", ":beetle:"]

[[symbol]]
char = "⟨$CURSOR⟩"                 # a template: the caret lands at $CURSOR
names = ["<>", "langle"]
languages = ["lean", "julia"]     # optional scope; absent = everywhere
```
Removing a built-in name: `names = ["-alpha"]` on the same `char` (a leading `-`) — so a user can free `\a` for something else without editing the bundled file. Conflicts (one name → two symbols) are allowed; the dropdown shows both, exact-name ties are ordered user > workspace > extension > built-in.

## Where it lives
- **Rust owns the table**: `moonkale-ext-api::unicode` — `Table::load(layers) -> Table`, `Table::complete(prefix, language) -> Vec<Match>` (ranked), `Table::names(char)`, hot-reloaded when a layer file changes (the settings watcher exists). One table per window, shared by every input.
- **Inputs ask Rust**: the CodeMirror bundle already has a completion channel to Rust (`onCompletion` → `completionResult`, Milestone 7); the `\` trigger becomes a second completion source in the same bundle (a `matchBefore(/\\[^\s\\]*/)` source that asks `onUnicode(id, text)` and applies the replacement, with the eager rule as a `keydown` filter). Milkdown (rich markdown) gets the same source through its own plugin API; plain Dioxus `input`/`textarea` elements (palette, search, chat, flow fields, table cells) get a **`UnicodeInput` wrapper component** in `ui` that listens to `oninput`, shows the dropdown as a positioned popover, and writes the replacement back through the same `Signal<String>` — so it is one implementation for every plain input. The terminal gets it via the palette command (xterm owns its keystrokes).
- **No language knowledge in JS** (rule of `packages/js`): the bundle only sends the text after the escape and applies what Rust returns.
- **Language scope**: the editor passes the document's language so Lean-only names (`\fun` → `λ` is fine everywhere, but `\<>` templates are Lean-flavoured) can be scoped by their table, not hard-coded.

## Settings
`input.unicode.enabled` (default on), `input.unicode.escape` (`\`), `input.unicode.escape_by_language` (default `{ latex = ";", markdown = ";", ipynb = ";" }` — the languages where `\` is source; `""` turns it off for a language), `input.unicode.eager` (on), `input.unicode.emoji` (on), `input.unicode.languages_only = []` (empty = everywhere; e.g. `["julia","lean","typst"]` to keep it out of Go files), plus the user table in *Settings → Input*.

## Interactions to get right
- **Languages where `\` is source — the exceptions**: LaTeX, Markdown (its `$\alpha$` math and `\*` escapes) and Jupyter notebooks (`.ipynb`, LaTeX in markdown cells) legitimately contain `\alpha` *as text*, and the renderer needs the backslash form. For these three the `\` trigger is **off by default**, and a **per-language escape** lets you keep the feature with another character (`;alpha` → α is what Lean users know): `input.unicode.escape_by_language = { latex = ";", markdown = ";", ipynb = ";" }`. An empty string disables it for that language.
- **Typst is not an exception**: Typst syntax does not use `\` (`\` is only a line break in content, and Typst renders Unicode directly — `α` and `∫` are valid source, `sym.alpha` is the long form). So Typst keeps the default `\` trigger and gets, through the Typst language table, names scoped to it (`\integral` → ∫ where Typst would spell `integral` — both work).
- Regex strings, Windows paths, C escapes: the "word boundary + not inside a string" rule plus eager-off inside string tokens once highlighting exists.
- **LSP completion** in the same editor: the two sources coexist; CodeMirror merges sources, and the `\` source only answers when the text before the caret starts with the escape.
- **Presence and history**: a replacement is an ordinary edit (one `Content` patch), nothing to do.
- **Phone**: the dropdown is the same popover; the soft keyboard's own suggestions stay.

## Plan (a milestone step, not a spec-sized change)
1. `ext-api::unicode` table + merge rules + tests (names → char, reverse, overrides, `-name`, templates) and the conversion script for the three source tables (licences kept).
2. Code editor: the `\` completion source and eager replacement in `packages/js/codemirror`, the Rust side in `editors/code`; E2E: type `\alpha` Tab in a `.jl` file → `α`; `\:ladybug:` → 🐞; `\int` in the dropdown shows both names.
3. `ui::UnicodeInput` wrapper on palette, search, chat, flow fields, table cells; E2E on the palette.
4. Milkdown source; hover reverse lookup; *Insert symbol…* command; Settings → Input table.
5. Extension contribution `unicode` + the Lean/Julia core language extensions ([[Core Languages]]) carrying their scoped tables.

Not in scope: OS-level input methods (pinyin etc. stay with the OS), font fallback for symbols the system font lacks (a theme concern — bundle a maths-capable fallback font later if needed).
