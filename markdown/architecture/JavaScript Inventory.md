---
title: "JavaScript inventory — what is left, and how far from zero"
description: Every place JavaScript/TypeScript still exists in Moonkale (four bundled packages, ~200 lines of inline eval scripts, Dioxus's own interpreter, dev-only tooling), what each would take to replace with Rust, and why node vs deno vs bun makes no difference to the shipped app.
tags: [architecture, interop, javascript, reference]
---
From [[Prompt12]] (2026-09-19). The Tauri-era frontend was Deno + Vite; this project is Rust with Dioxus, and JavaScript survives only where a Rust replacement does not exist yet. Principle: [[ADR-0002 Rust first, TypeScript behind traits]]; mechanism: [[JS Interop Boundary]]; candidates: [[Rust-native Editor Candidates]]. **Keep this page current when a bundle is added or removed.**

## How Dioxus uses JavaScript (so the rest makes sense)
- **Web build**: the app is Rust compiled to **wasm** and runs in the browser's engine. Dioxus does not render through a JS framework: Rust produces a binary stream of DOM mutations that a small JS shim (`dioxus-interpreter-js`, a few KB, part of Dioxus 0.7.10) applies; DOM events go back into wasm. That shim is the *only* JavaScript Dioxus itself needs.
- **Desktop and mobile builds**: the app is a **native Rust process**; the UI is the platform's **webview** (WebKitGTK on Linux, WebView2 on Windows, WKWebView on macOS, the Android WebView) with the same interpreter shim loaded into the page and an IPC channel (`wry` 0.53) between them. `document::eval` runs a JS string in that page — that is how our bundles are mounted.
- **Server build**: no JavaScript at all (server-side rendering produces HTML; the client's wasm hydrates it).
- **No Node, Deno or Bun ships with Moonkale on any platform.** The JS engine at runtime is always the browser's or the webview's (JavaScriptCore on Linux/macOS/iOS, V8/Blink on Windows and Android).

## The inventory (2026-09-19)

| where | what | size (built) | why it exists | Rust replacement — status and distance |
|---|---|---|---|---|
| `packages/js/codemirror` → `editors/code/assets/codemirror.js` | CodeMirror 6 (state, view, commands, search, autocomplete, lint, one-dark theme) + **grammars** for highlighting (P-093, spec [[010]]); ≈ 400 lines of our TypeScript | 1 021 KB (368 KB before the grammars) | the code editor's text view: cursor, selection, IME, scrolling, gutters, search panel, completion popup | **medium.** Rust already owns the document (rope, versions, patches — [[ADR-0008 Rust owns the document, JS is a view]]), LSP and history; JS is a view. Candidate: a virtualised Dioxus text view (rows as DOM lines, own cursor/selection). Hard parts: IME composition, accessibility, bidi, mobile soft-keyboard behaviour — the reasons CodeMirror is 300 KB. Highlighting does not depend on it (none exists yet — [[Core Languages]]) |
| `packages/js/milkdown` → `editors/markdown/assets/milkdown.js` | Milkdown (Crepe + Kit) on ProseMirror + KaTeX; ≈ 300 lines of ours (wrapper + the wiki-link plugin) | 2 750 KB + `assets/katex/` (css + 20 fonts) — the largest asset in every build | the WYSIWYG markdown editor | **far.** No Rust WYSIWYG markdown editor exists; the path is a Dioxus block renderer with per-block text editing (the code editor view reused inside blocks). Realistically the last to go |
| `packages/js/xterm` → `editors/terminal/assets/xterm.js` (+ `xterm.css`) | xterm.js 6 + fit addon; 94 lines of ours | 339 KB | terminal grid rendering, keyboard, selection | **near.** `alacritty_terminal` gives the grid/parser in Rust; drawing it as a virtualised Dioxus grid is bounded work. First to replace (as [[Rust-native Editor Candidates]] says) |
| `packages/js/wasm-host` → `ui/assets/wasm_host.js` | our own 104 lines, no dependencies | 3 KB | runs wasm extensions **in the browser** (Worker + `SharedArrayBuffer` mailbox) — there is no Rust on the web client that could host them | **not replaceable on the web** by definition (the browser *is* JS + wasm); **not needed on desktop/server** (wasmtime). Stays as long as web extensions exist |
| `editors/graph/assets/graph_render.js` | wasm-bindgen glue for our Rust renderer (generated, not written) | 128 KB (+ 2.4 MB wasm) | a `<canvas>` inside a webview can only be reached through JS; the renderer itself is Rust/wgpu | **goes away only with `dioxus-native`** (a native wgpu surface instead of a webview canvas) — [[ADR-0011 Desktop graph surface strategy]] |
| inline eval scripts in Rust (`r#"…"#` strings) | ≈ 200 lines across `graph/panel.rs` (66), `code/backend/codemirror.rs` (32), `terminal/panel.rs` (24), `markdown/rich.rs` (22), `ui/frame.rs` (31), `ext-api/workspace.rs` (15), `ui/shell.rs` (8), `web/main.rs` (5) | — | mounting the bundles, `ResizeObserver`s, focus/scroll, the stylesheet insertion (P-087), key routing, the login page | each disappears with its bundle; the shell ones (focus, scroll-into-view, resize) become Dioxus APIs as they land |
| `dioxus-interpreter-js` (inside Dioxus) | Dioxus's DOM shim | a few KB | see above | **only with `dioxus-native`** (Blitz: Dioxus rendering to wgpu without a DOM) |

**Not JavaScript**: Typst preview (the `typst` crate, SVG from Rust), the flow editor (`dioxus-flow`, Rust), the graph layout and renderer (Rust → wasm), LSP, terminal PTY, index, search, git, history, presence, the agent, DuckDB/SQLite/LadybugDB. Everything that *thinks* is Rust; JavaScript only *draws* text and terminals.

**Development-only** (never shipped): esbuild + TypeScript to build the four bundles (`npm run build`, output committed so `dx` needs no Node); Playwright E2E (29 suites, ≈ 2 100 lines of `.mjs`); the Quartz site (Node, ≈ 60 npm packages — it is the documentation, not the product); the `cdp.mjs` helper for the Android WebView.

## How far from zero
Counted in *what the desktop would still need a JS engine for*:

1. **Terminal** — bounded; `alacritty_terminal` + a Dioxus grid. Removes 339 KB + `xterm.css`.
2. **Code editor** — the virtualised view; the design problems are IME and accessibility, not text. Removes 367 KB. Do highlighting (tree-sitter decorations) first, it is needed either way.
3. **Rich markdown** — the long one; until then Milkdown stays and is 2.7 MB of the 3 MB of JS.
4. After 1–3, the desktop still loads Dioxus's interpreter shim and the wasm-bindgen glue for the graph canvas — i.e. it is still a **webview app**. Zero JS on desktop means **`dioxus-native`** (Blitz), which also gives the graph a real surface. That is a platform move, not an editor rewrite; the web build keeps the browser's JS engine forever by definition (and `wasm_host.js` with it).

So: three replacements in the order terminal → code → rich text, then a renderer switch. None is scheduled; the boundary rules (no state, no language knowledge in JS, `PROTOCOL.md` per package) are what keep each one a bounded job.

## node, deno or bun?
**Used today: Node (v26.9) with npm.** Deno is installed on this machine and unused; Bun is not installed.

For **the shipped app** the choice makes **no difference whatsoever** — none of them is part of it. Speed, memory, startup time and security of Moonkale are those of the Rust binary and the platform webview; the JS bundles are static files the webview parses, and they would be byte-identical from any of the three runtimes because the bundler is **esbuild** (a Go binary) in every case.

Where the choice does matter, it is developer tooling only:

| | Node + npm | Deno | Bun |
|---|---|---|---|
| building the four bundles | works (esbuild) | works (`deno run -A npm:esbuild`, or `deno task`), same output | works, same output; fastest installs |
| Playwright E2E | required — Playwright's browser drivers target Node | mostly works via npm compat, not officially supported | mostly works, not officially supported |
| Quartz (site) | required (uses Node APIs: `sharp`, workers, fs) | no | partial |
| install speed | slowest | fast | fastest |
| supply-chain posture | `npm install` runs package scripts with full user rights | **permissions model** (`--allow-*`), scripts do not run by default, lockfile with integrity | scripts run by default, lockfile |
| TypeScript without a build step | no (we use esbuild anyway) | yes | yes |

Practical stance: **keep npm** because Playwright and Quartz need Node regardless, so a second runtime would be a second toolchain for the same four `npm run build` commands. If supply-chain safety of the *build* is the concern, the cheap improvement inside npm is `npm ci --ignore-scripts` for the bundle folders (none of our dependencies needs install scripts) — that is Deno's main advantage without the second runtime. Nothing here changes bundle size, startup, memory or security of Moonkale itself; those come from the table above, i.e. from how much JavaScript is still loaded into the webview, and Milkdown's 2.7 MB is the one number worth attacking.

## Milestone 14 (2026-09-21): the code editor has a Rust twin
`editors/code-native` renders and highlights without any bundle — `dioxus-code-editor` + `arborium` (tree-sitter compiled to Rust/wasm) — and the index shares that runtime now (P-113). Distance to removing CodeMirror: an editor *engine* (cursor, decorations, completion popup, search, fold) on the Rust side; the crate is a textarea plus a highlighter. One JavaScript touch remains in the Rust editor: reading the textarea's `selectionStart` through Dioxus's eval for the caret. Terminal (Milestone 12) and code editor (Milestone 14) now each have a JS-free opt-in twin; the rich-text editor (Milkdown) is the last without one. See [[Code Editor Implementations]].
