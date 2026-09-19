---
title: "Milestone 7 — Daily Driver: the plan"
description: What it takes to use Moonkale on its own repository every day — command palette and quick open, file operations, find/replace, LSP completion and rename, git as a first-class source, and a web server that can be exposed.
tags: [milestone, planning]
---
**Goal**: after six milestones every big bet has a working answer (graph model, wgpu view, sources, agents, settings, extensions, flow editor, scale, phone). What is missing is the unglamorous half of "editor": the things one reaches for every ten minutes. Daniel's observation (2026-09-19) that the project now needs "lots of little specifications" points the same way. Record: [[Milestone 7 - Implementation Log]]. Small requests keep going to [[specifications/README|specifications]].

The [[Roadmap]]'s Phase 7 items (3D, browser wasm extensions, JS-free desktop) are research with no user waiting for them; they stay deferred and are listed at the end with what a spike would cost. This milestone instead takes the two ranked problems that block daily use — **P-32 git** and **P-20 secure remote** — plus editor completeness (P-14's second half) and the workspace commands that every extension will need (P-05's second half).

## Starting point (after Milestone 6)
- Commands: a closed `Command` enum in `ext-api`, dispatched by menus and a hard-coded shortcut table in `ui::frame` (`Ctrl+W/`/N/O/Shift+F/,`). Extensions cannot contribute commands; there is no palette; no quick-open.
- Explorer: browse, open, reveal; no create/rename/delete/move. `core::Op` has `WriteText` and `CreateText` only.
- Search: workspace search (BM25 + vectors) opens hits; no replace; the CodeMirror bundle has no search panel (only default/history keymaps + F12).
- LSP: diagnostics, hover, go-to-definition (cross-file via `reveal`). No completion, rename, code actions, references.
- Git: none. Folders that are repositories are just folders; `.moonkale/` and `.gitignore` are respected by the walker (`ignore` crate).
- Web server: every relay (terminal, LSP, LLM, wasm, MCP) is dev-server-only, no auth (P-20); `MOONKALE_MCP_TOKEN` is the only gate.
- Machine: `git` 2.55, `rust-analyzer` present; no second machine to test remote access from (curl + a second browser profile stand in).

## Scope: what "done" means
1. **Commands as a contribution point** — `Extension::commands() -> Vec<CommandContribution { id, title, keybinding, when }>` executed through `Extension::run_command(id, ws)`; the built-in `Command` enum becomes the first contributions (`workspace.open_folder`, `file.save`, …); one keybinding table (settings-overridable, `keybindings` in `SettingsFile`) replaces the shortcut function; menus are built from commands. **Command palette** (`Ctrl+Shift+P`): fuzzy over titles, shows keybindings, runs. **Quick open** (`Ctrl+P`): fuzzy over the folder's files (index-backed), opens; `:line` suffix reveals.
2. **File operations** — `core::Op::{CreateDir, Rename, Delete}` (+ `CreateText` already) implemented by the folder source (native and `RemoteSource`), with tombstone-free semantics (delete = move to the OS trash on desktop, `.moonkale/trash/` on the server); Explorer context menu (right-click / long-press): New File, New Folder, Rename (inline), Delete, Reveal in terminal; drag a row onto a folder to move. Open documents follow renames (`Document.node` updates through a `Renamed` event); the index reindexes the affected paths.
3. **Find & replace** — in-file: CodeMirror's search panel in the bundle (`Ctrl+F`, `Ctrl+H`, regex, case, whole word) with the replacements flowing back as ordinary patches; workspace: the Search panel gets a *Replace* row that previews per-hit diffs and applies through `WriteText` (respecting versions), with the same approval card style as agent edits.
4. **LSP, second half** — completion (`textDocument/completion` → CodeMirror `autocompletion` with the LSP kind icons, resolve on demand), rename (`textDocument/rename` → multi-file `WorkspaceEdit` applied through documents/`WriteText`, previewed), code actions (`Ctrl+.`, quick fixes only), references (`Shift+F12` → results in the Search panel). `lsp` crate stays feature-neutral; remote LSP unchanged.
5. **Git** (P-32) — a `vcs-git` extension (optional, on by default when the folder has `.git`): status decorations in the Explorer and on tabs (M/A/U colours), a *Changes* panel (staged / unstaged, click → diff), diff view (CodeMirror merge view, side-by-side or inline), stage/unstage/discard, commit with message, branch switch, log; **history as a graph** in the Graph panel (commits → files touched, `Query::All { kinds: [Commit] }`) — the first source that is not files or a database. Implementation via the `git` CLI through a small typed wrapper (porcelain v2), native on desktop and server-side on web; `gix` later if the CLI proves limiting.
6. **Secure remote** (P-20) — the web server gets an access token (`MOONKALE_TOKEN`; generated and printed at start when unset and not on loopback), a login page that stores it as an HttpOnly cookie, and a check on every server function and websocket; relays refuse without it; rate/size limits on terminal output and LLM calls; audit line per relay call. Binding to a non-loopback address without a token refuses to start. `dx serve` on loopback keeps working with no token (dev mode, printed once).
7. Web parity for all of it; E2E; documentation.

**Deferred** (documented): the entity log (P-33, ADR-0012 stays *proposed* until git is in), collaboration/presence (P-30/P-34), Postgres/Turso (P-19), TypeDB/Helix (P-26), 3D (P-27), browser-side wasm extensions (P-28 — note: with the JSON ABI on core modules a browser runtime is now a Worker + `WebAssembly.instantiate`, no component layer needed; a 1-day spike when wanted), JS-free desktop (P-29), OS keychain, MTK library, execute-in-place Julia.

## Architecture decisions for this milestone

```mermaid
flowchart LR
  EXT[extensions: commands()] --> REG[command registry + keybindings]
  REG --> PAL[palette / quick open] & MENU[menus] & KEYS[global keys]
  OPS[core::Op CreateDir/Rename/Delete] --> FS[folder source native] & RS[RemoteSource → api]
  FS --> EXP[explorer context menu, drag-move]
  LSP[lsp: completion/rename/actions/refs] --> CM[codemirror bundle: autocompletion, search]
  GIT[vcs-git: git CLI wrapper] --> CH[Changes panel + diff] & GG[commit graph in Graph panel] & DEC[explorer/tab decorations]
  TOK[(MOONKALE_TOKEN)] --> API[api: cookie login, per-call check, limits, audit]
```

- **Commands are data**: `{ id, title, keybinding, when }` contributed like panels; the shell owns one registry and one keybinding table; the `Command` enum survives as the payload of `ws.dispatch` for built-ins, extension commands run through the trait. Settings can rebind (`keybindings: { "file.save": "Ctrl+S" }`).
- **File operations are transactions** on the source, never direct filesystem calls from the UI, so the remote source, the index and open documents all see the same event.
- **Replace is a patch**: every replacement — in-file or workspace-wide — becomes a `TextPatch` on a `Document` with a version check; nothing rewrites files behind the editor's back.
- **Git via the CLI** (porcelain v2, `--porcelain` everywhere, `-z` separators): zero build cost, matches what users expect of their repo config (hooks, signing, credential helpers); a `Vcs` trait keeps `gix` possible. Server-side on web like every other tool relay.
- **Commits are nodes**: the git source implements `Source` with `NodeKind::Commit` and `Touches` edges to files, so the Graph panel, the agent (`graph.query`) and MCP see history without new UI.
- **Token first, accounts later**: one shared token with a cookie is what a single-user server behind a reverse proxy needs; user accounts are out of scope. Every relay checks the same `Authz` extractor.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | command contributions + registry + keybindings + palette + quick open; built-ins migrated; menus from commands | ext-api, ui, editors/* | unit: fuzzy ranking, keybinding parse; E2E: palette runs a command, quick open reveals `:line` |
| 2 | `Op::{CreateDir, Rename, Delete}` in core/project-fs/api/remote; explorer context menu, inline rename, drag-move; documents follow renames; index reindexes | core, project-fs, api, ext-api, ui, index | unit: ops + trash; E2E: create/rename/delete/move on web |
| 3 | in-file search panel (bundle), workspace replace with preview | js/codemirror, editors/code, ui/search | E2E: replace in file and across two files |
| 4 | LSP completion, rename, code actions, references | lsp, editors/code, js/codemirror | E2E with rust-analyzer: completion popup, rename across two files |
| 5 | `vcs-git`: wrapper, status, Changes panel, diff, stage/commit/branch/log, decorations, commit graph source | extensions/git (new), api, ui, editors/graph | unit: porcelain parsing; E2E: modify → status → stage → commit → graph shows the commit |
| 6 | token auth + cookie login + limits + audit on `api`; refuse non-loopback without a token | api, web | E2E: relay refused without cookie, works after login; curl checks |
| 7 | verify, log, vault | | |

## Risks
| risk | mitigation |
|---|---|
| Command migration touches every menu/shortcut path | keep `Command` enum as payload; migrate menus last, behind the same tests (`menubar.mjs`) |
| Rename while a document is open and dirty | documents are keyed by `NodeId` (uuid of path) — a rename changes the id; the source emits `Renamed { from, to }` and the workspace re-keys the document, keeping text and version |
| CodeMirror merge view size in the bundle | `@codemirror/merge` is small; lazy-load the diff view chunk if the bundle grows > 1 MB |
| git CLI output variations | porcelain v2 with `-z` is stable across versions; tests parse recorded output |
| Token in the browser | HttpOnly cookie, SameSite=Strict, never in localStorage or the URL after login; the MCP endpoint keeps its own bearer |
| rust-analyzer completion latency | debounce 150 ms, cancel superseded requests (`$/cancelRequest`), show what is cached |
