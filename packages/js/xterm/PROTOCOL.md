# @moonkale/xterm protocol

`window.moonkale.xterm`:

| function | effect |
|---|---|
| `mount(el, {onData, onResize}) → {cols, rows}` | create a terminal in `el`, fit it, watch its size |
| `write(el, base64)` | feed process output |
| `focus(el)`, `fit(el)` | |
| `lineAt(el, clientY) → string[]` | text of the row under the pointer and its two neighbours (Rust runs link detection on them) |
| `destroy(el)` | |

Messages Rust → JS: `{kind:"output", data:<base64>}`, `{kind:"focus"}`, `{kind:"destroy"}`.
Messages JS → Rust: `{kind:"ready", cols, rows}`, `{kind:"input", data:<base64>}`,
`{kind:"resize", cols, rows}`, `{kind:"link", lines}` (Ctrl+click).
All payload bytes are base64: binary-safe through JSON.

## Milestone 4
`allText(el) → string` — the whole buffer (scrollback + screen) joined by newlines, for trace parsing. Rust → JS `{ "kind": "text" }` asks for it; JS → Rust `{ "kind": "text", "text": "…" }` answers.
