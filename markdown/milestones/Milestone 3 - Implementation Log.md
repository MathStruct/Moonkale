---
title: "Milestone 3 — Implementation Log"
description: What was built for "Databases and Tools", what deviated from the plan, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 3 - Databases and Tools]].

> [!success] Done (2026-09-18)
> All six steps are implemented and verified on the web build with Playwright; desktop compiles and shares every code path except the three spawners (in-process PTY, stdio language server, in-process Typst), which are covered by native tests. **Terminal** (xterm.js view, PTY on desktop, websocket relay on web, Ctrl+click on `path:line`), **Typst preview** (live SVG pages next to a `.typ` editor), **LSP** (rust-analyzer: diagnostics in the gutter, hover, F12; status bar item), and **LadybugDB** (embedded Cypher database as a source: schema in the Explorer, Cypher in the table editor, *Show in Graph* draws the result). Four new E2E suites (`terminal`, `typst`, `lsp`, `ladybug`) plus the M1/M2 suites pass; 7 new native tests; clippy/fmt clean on every target.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `moonkale-terminal` + `moonkale-terminal-pty` + `js/xterm` bundle + `editors/terminal` panel; desktop PTY | ✅ pty test | [terminal.md](https://github.com/MathStruct/Moonkale/blob/master/packages/terminal/terminal.md), [terminal-pty.md](https://github.com/MathStruct/Moonkale/blob/master/packages/terminal-pty/terminal-pty.md), [editor-terminal.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/terminal/editor-terminal.md) |
| 2 | `/api/terminal` websocket + `RemoteTerminal`; "new terminal here" | ✅ E2E `terminal.mjs` | typed `Websocket<TerminalMessage, TerminalMessage>` from `dioxus::fullstack` |
| 3 | `moonkale-typst` World + `compile_to_svg`; `compile_typst` server fn; preview panel | ✅ 2 tests + E2E `typst.mjs` | [typst.md](https://github.com/MathStruct/Moonkale/blob/master/packages/typst/typst.md) |
| 4 | `moonkale-lsp` session + `moonkale-lsp-local` stdio/discovery; CodeMirror lint/hover/F12; `/api/lsp` relay; status bar | ✅ 2 tests (one against real rust-analyzer) + E2E `lsp.mjs` | [lsp.md](https://github.com/MathStruct/Moonkale/blob/master/packages/lsp/lsp.md), [lsp-local.md](https://github.com/MathStruct/Moonkale/blob/master/packages/lsp-local/lsp-local.md), [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) |
| 5 | `sources-graph::ladybug` (`lbug` 0.20); Explorer/`open_any` detection; Cypher in the table editor; Graph panel source picker + *Show in Graph* | ✅ integration test + E2E `ladybug.mjs` | [sources-graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-graph/sources-graph.md), [graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph/graph.md) |
| 6 | verify, log, vault | ✅ | this note |

## What the user sees
- **Terminal** tile at the bottom (View → New Terminal, Ctrl+`, `+`, or `>_` on a folder in the Explorer). Ctrl+click on `src/main.rs:12:4` in the output opens the file.
- Open a `.typ` file → a **Typst preview** tab appears next to it and follows every keystroke.
- Open a `.rs` file in a cargo project → the status bar shows `rust: starting…` → `rust-analyzer: ready`; errors get a red gutter marker with the message on hover; hovering a symbol shows its signature; F12 jumps to the definition. Without rust-analyzer: `rust: no language server for rust (install: rustup component add rust-analyzer)`.
- A `people.lbug` in the folder shows in database colour; clicking it adds a **PEOPLE.LBUG** source with its node and rel tables; a table opens the query box with `MATCH (n:Person) RETURN n LIMIT 200`; a path query shows **Show in Graph**, and the Graph panel's new source picker switches to the database and draws the vertices and relations (schema graph when the *query* box is unticked).

## Deviations from the plan
1. **Ladybug needed no cmake.** `lbug` 0.20 downloaded its prebuilt static library on first build (~15 s); the binary grows by ~40 MB. The `ladybug` feature is on for desktop and the server; the crate still compiles without it.
2. **The desktop app never opened `.sqlite` files** — Milestone 2 only wired that on the server. Now `open_database` in `desktop/src/main.rs` handles SQLite and Ladybug alike.
3. **Hover is plain text.** The tooltip is a `<div>`; markdown fences from rust-analyzer are stripped in `lsp::hover_text`. Rendering markdown in tooltips waits for the Milkdown work.
4. **Go-to-definition across files opens the file but does not position the cursor** — `open_relative_path` has no "then move the cursor" hook yet.
5. **Language servers are keyed per (language, folder root)**, not per file, and `initialize` is sent with that root; rust-analyzer needs a `Cargo.toml` under it.
6. **Terminal `Ctrl+click` needs the neighbour rows**: xterm wraps long paths, so the panel asks for the row above and below and joins them before link detection.
7. **The `/api/lsp` websocket carries a `Frame(String)` newtype** rather than `String` — the server-function macro cannot handle a bare `String` type parameter (P-057).
8. **Typst packages (`@preview/...`) are unsupported** — no download path in `MoonkaleWorld` yet.
9. **Graph panel: index filters hide when a database is picked** (files/symbols/folders/unresolved are index kinds); database drawings show every kind.

## Problems hit (→ [[Problem Log]])
- **P-054 Typst preview recompiled itself forever**: the effect read `busy()` (reactive) and set it, so each compile scheduled another; `busy.peek()` fixed it.
- **P-055 Terminal link clicks**: (a) the row under the mouse must be computed from `.xterm-rows` geometry, not the outer element; (b) xterm and headless Firefox swallow a real Ctrl+click, so the E2E dispatches a synthetic `click` with `ctrlKey`.
- **P-056 `dx serve` restart cadence** — still ~2–3 min per non-rsx change; three restarts this milestone.
- **P-057 `Websocket<String, String>` does not compile in a server function** — the macro turns the parameter into `str`; wrap the payload in a newtype.
- **P-058 rust-analyzer progress messages are file paths** — `$/progress` messages like `13/15: /home/…/core` overflowed the status bar; cut at the first `: `.
- **P-060 Database internals invisible in the graph view** (found by Daniel on desktop): the picker defaulted to *index* and *Show in Graph* didn't raise the Graph tab. Opening a graph database now selects it; `Command::ShowPanel` brings the tab forward.
- **P-061 WebKitWebProcess crashes in NVIDIA EGL at desktop start** (core dumps reported by Daniel): WebKitGTK's DMA-BUF renderer vs. the proprietary driver; the launcher now sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` when the NVIDIA driver is loaded.
- **P-062 Only the schema was drawn for a database**: added Data/Schema/Query modes with per-label colours and a legend; `Query::All { kinds: [Vertex] }` is the source-side contract for "the data graph".
- **P-059 E2E fixtures drift**: the graph suite saves a wiki-link to `Another` into `Home.md`, which then breaks the links suite's counts. Suites must be run on a fresh fixture (or restore `Home.md` between runs) — noted in the E2E README.

## Decisions worth keeping
- **Typed websockets for tools** (`dioxus::fullstack::{WebSocketOptions, Websocket}`): server `options.on_upgrade(|mut socket| …)`, client `.send/.recv` on a shared `Rc` — the same shape for the terminal and the LSP relay; `Frame` newtype for strings.
- **`SpawnTerminal` / `SpawnLsp` / `CompileTypst` are plain `fn` pointers in `WorkspaceConfig`**, like `open_folder`; `None` on mobile. Platforms decide where the process runs; the panels never know.
- **Query results carry nodes/edges *and* a table.** `QueryResult` already had both; Ladybug fills both from one Cypher run so the table editor and the graph panel share a result without a second query.
- **`Workspace::graph_request`** is the one-way channel from any panel to the Graph panel ("draw this"). A checkbox in the panel lets the user go back to the schema.
- **Security stays explicit**: `MOONKALE_ROOT` jails cwd/root for terminal, LSP and databases on the server; nothing else is hardened (P-20).

## Verified
- Web (Firefox, Playwright): `terminal.mjs`, `typst.mjs`, `lsp.mjs`, `ladybug.mjs`, plus M2's `graph.mjs`, `links-sqlite.mjs`, M1's `session.mjs`, `menubar.mjs` — all PASS against `MOONKALE_ROOT=<fixture>` with rust-analyzer installed.
- Native: `cargo test` for `terminal`, `terminal-pty` (real PTY), `typst`, `lsp`, `lsp-local` (real rust-analyzer), `sources-graph --features ladybug` (real database).
- Desktop: `cargo build -p desktop --features desktop` links (with `lbug` static lib). Not exercised by hand this session (no display in the agent's shell) — see below.

## What to look at on desktop
1. View → New Terminal: a shell in the bottom tile; `ls` output; Ctrl+click a `path:line`.
2. Open a `.typ` file: preview tab appears and updates while typing.
3. Open `src/*.rs` in a cargo project: status bar `rust-analyzer: ready` after indexing; a deliberate `let x: String = 1;` gets a gutter marker; hover; F12.
4. File → Open Folder… on a `.lbug` file or directory (or click one inside a folder): tables appear; Cypher runs; *Show in Graph*.

## Deferred to Milestone 4
Milkdown WYSIWYG (P-11), Postgres/Turso/DuckDB, Falkor/TypeDB/Helix, cross-source edges (P-16), cursor positioning after cross-file go-to-definition, Typst packages, LSP completion/rename, terminal/LSP auth for a real server, wasm-side Typst.
