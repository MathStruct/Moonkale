---
title: "Milestone 7 — Implementation Log"
description: What was built for "Daily Driver", what deviated from the plan, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 7 - Daily Driver]].

> [!success] Done (2026-09-19)
> All seven steps are implemented and verified on the web build; desktop builds and shares every code path (git runs in-process there). **Commands are a contribution point**: extensions list `{id, title, keybinding}`, the shell keeps one registry with settings-overridable keybindings (Settings → Keybindings), menus show the effective shortcut, and a **command palette** (`Ctrl+Shift+P`) plus **quick open** (`Ctrl+P`, `:line` suffix) drive them. **File operations**: `Op::{CreateDir, Rename, Delete}` on the folder source (delete = `.moonkale/trash/`), Explorer context menu with inline New File / New Folder / Rename, drag a row onto a folder to move, open documents follow renames with their unsaved text, the index follows too. **Find & replace**: CodeMirror's search panel in every editor (`Ctrl+F` / `Ctrl+H`) and a workspace replace with a per-file preview (open documents stay unsaved, closed files are written through the source). **LSP second half**: completion, `F2` rename (multi-file `WorkspaceEdit`), `Ctrl+.` code actions (with `codeAction/resolve`), `Shift+F12` references — verified against rust-analyzer. **Git**: a *Changes* panel (branch, staged/unstaged, stage/unstage/discard, commit, log), diff views, Explorer and tab decorations, and **history as a graph** in the Graph panel; the `git` CLI runs on desktop in-process and on the web server. **Access token**: `MOONKALE_TOKEN` gates every server function and relay (HttpOnly cookie via `/login`, or a bearer), audit lines per relay call, login rate limit, and the server refuses a non-loopback bind without a token. Six new E2E suites (`palette`, `files`, `replace`, `lsp2`, `git`, `auth`) plus the seventeen earlier ones pass; 74 native tests; clippy/fmt clean.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `CommandContribution`, `Keybinding` (parse/match/display), `fuzzy_score`; `Extension::{commands, run_command}`; `ui::commands::Registry` (built-ins + enabled extensions + `settings.keybindings`); palette + quick open; menus from the registry; Settings → Keybindings; `agent.focus` as the first extension command | ✅ 2 unit tests, E2E `palette.mjs` | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md), [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 2 | `core::Op::{CreateDir, Rename, Delete}`; folder source (trash), `RemoteSource` unchanged (transactions pass through); `Workspace::{create_dir, rename_node, delete_node}` re-key documents; index `refresh` removes subtrees, walks new directories, re-resolves links; Explorer context menu, inline edits, drag-move, `fs_epoch` reloads | ✅ 2 unit tests, E2E `files.mjs` | [project-fs.md](https://github.com/MathStruct/Moonkale/blob/master/packages/project-fs/project-fs.md), [index.md](https://github.com/MathStruct/Moonkale/blob/master/packages/index/index.md) |
| 3 | `@codemirror/search` in the bundle (`Mod-h` too), minimal-diff `setText`; Search panel replace row + preview; `Workspace::{count_occurrences, replace_in_file}` | ✅ E2E `replace.mjs` | [markdown.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/markdown/markdown.md) unaffected; [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) |
| 4 | `lsp`: `completion`, `rename`, `code_actions` + `resolve_code_action`, `references`, `WorkspaceEdit` (parse both shapes, apply last-to-first, UTF-16 columns), `strip_snippet`; bundle: `autocompletion` + `F2` / `Mod-.` / `Shift-F12` hooks; panel: rename prompt, actions bar, references list; `apply_workspace_edit` | ✅ 2 unit tests, E2E `lsp2.mjs` (rust-analyzer) | [lsp.md](https://github.com/MathStruct/Moonkale/blob/master/packages/lsp/lsp.md), [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) |
| 5 | `ext-api::git` types + porcelain-v2/log parsers; `extensions/git` (`cli` runner, Changes panel, diff panel, history source, commands `git.refresh/commit/history`); `api::git_run`; desktop/web wiring; `Workspace::vcs_status` → Explorer + tab decorations; Graph panel draws `Custom("git")` sources | ✅ 4 unit tests, E2E `git.mjs` | [git.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/git/git.md) |
| 6 | `api::auth`: token, cookie login, bearer, audit target `moonkale::audit`, login rate limit, `bind_mode` guard; wired in `web/src/main.rs` | ✅ 2 unit tests, E2E `auth.mjs` (own server on 8091) | [api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/api/api.md) |
| 7 | verify, log, vault | ✅ | this note |

## What the user sees
- `Ctrl+Shift+P` lists every command with its shortcut; `Ctrl+P` jumps to a file (`main.rs:12` opens at line 12). Settings → Keybindings rebinds any of them (empty = unbound); menus follow.
- Right-click in the Explorer: New File…, New Folder…, Rename…, Delete…, Open in Terminal. Drag a file onto a folder to move it. Deleted files sit in `<folder>/.moonkale/trash/<timestamp>/` until you empty it.
- `Ctrl+F` / `Ctrl+H` inside an editor; Search panel → *Replace with…* → Preview → Replace all.
- In a Rust file with rust-analyzer: type to complete, `F2` to rename across files (unsaved edits, save with `Ctrl+S`), `Ctrl+.` for assists, `Shift+F12` for references.
- Changes tab: stage with `+`, discard with `✕`, commit with `Ctrl+Enter`; click a change for its diff; **Graph** draws the last 200 commits and the files they touched in the Graph panel. Modified files are coloured in the Explorer and get a letter on their tab.
- To expose the web server: `MOONKALE_TOKEN=<long random string> dx serve --addr 0.0.0.0 …` (or the release binary behind a reverse proxy that sets `X-Forwarded-Proto`/`X-Forwarded-For`); open `/login` once per browser; scripts use `Authorization: Bearer`. Without a token the server only starts on loopback.

## Deviations from the plan
1. **Undo/Redo stay unbound in the registry** — CodeMirror owns those keys; the menu entries dispatch the commands as before. Keys reserved by the browser (`Ctrl+R/T/L/C/V/X/A/Z/Y`, `F5`, `F12`) are never forwarded by the document-level listener.
2. **Delete is a move to `.moonkale/trash/`** on every platform rather than the OS trash: no extra dependency, identical on the server, and the folder already git-ignores `.moonkale`.
3. **Rename changes the node id** (ids derive from paths); `rename_node` re-keys the open `Document` and the `views` entry in place, so text, dirty state and version survive and the tab simply changes its title. Editors remount (the panel id contains the node id).
4. **Workspace replace works on the files the search found**, counting literal occurrences there (the search itself is BM25/semantic); open documents receive the replacement as an unsaved edit, closed files are written with a version check and re-indexed. The plan's "diff per hit" became a per-file count with the open/closed distinction shown.
5. **`setText` in the CodeMirror bundle now replaces only the changed middle** (common prefix/suffix kept) so the cursor and scroll survive external edits (renames, code actions, agent edits) — a small change that helps everywhere.
6. **Code actions ask with an empty diagnostics context**; rust-analyzer offers assists on the selection (extract into variable/constant/static/function) and quick fixes where it has them. Bare `Command`s without an edit are filtered out (they would need `workspace/executeCommand`).
7. **Git via the CLI, porcelain v2 with `-z`** as planned; `gix` was not needed. Diffs are rendered as coloured unified text rather than a CodeMirror merge view — simpler, and `Open file` is one click away. The history source draws commits ↔ files (`touches`) and commit ↔ parent edges; double-clicking a commit shows its subject in the status bar.
8. **`git` and `ext-api` types**: `api` now depends on `ext-api` (no cycle) so the server function and the client share `GitRequest`/`GitResponse`.
9. **Client address in the audit log** comes from `X-Forwarded-For` when a proxy sets it; dioxus's `serve` does not install `ConnectInfo`, so a direct connection logs `?` and the login rate limit is then global (10 attempts/minute) rather than per address.
10. **The Changes panel sits in the side tile by default** (`explorer, search, links, git`): a panel missing from the saved layout is attached *and activated* by the workbench, which would have hidden the Explorer on every start.

## Problems hit (→ [[Problem Log]])
- **P-076 A task spawned in a component that unmounts in the same handler is dropped**: the inline rename/new-file field set `edit = None` (unmounting itself) and then `spawn`ed the operation — nothing ran. `spawn_forever` for work that outlives the component.
- **P-077 `#[serde(rename_all)]` on an enum does not rename variant fields**: the CodeMirror bridge's `codeActions` message carried `endLine` while Rust expected `end_line`; the deserialisation error ended the bridge's receive loop, taking every later editor event with it. Fields are now renamed per variant and an unreadable message is logged and skipped instead of breaking the loop.
- **P-078 A completion source that never sends its request**: the bundle registered the pending resolver but forgot to call `features.onCompletion` — found with a standalone HTML page loading the bundle, which is now the quickest way to debug JS-side behaviour (`~/.cache/moonkale-e2e/e2e/cm-test.html`).
- **P-079 Hydration race after a form redirect**: after `/login` the app page is server-rendered first; clicking *Open* before hydration does nothing. Tests wait for `networkidle` after the redirect; users are slower than Playwright.
- **P-080 cargo-check vs native diagnostics**: the fixture's type error is a cargo-check diagnostic; rust-analyzer republishes only native diagnostics on `didChange`, so the M3 suite's "fix clears the gutter" needed a save — and the code panel's `Ctrl+S` did not send `didSave` (only the toolbar button did). One `save_now` callback now serves Ctrl+S, the menu and the button.
- **P-081 load in long batches**: `phone` and `auth` timed out once late in the 23-suite batch and pass alone and in pairs; rerun before debugging.
- **P-059 again**: suites now also reset the git repository (`reset --hard <root commit>` + `clean`) and the files `files.mjs` creates.
- `dx serve` still needs a restart after asset rebuilds (P-070) — twice this milestone for the CodeMirror bundle.

## Decisions worth keeping
- **One registry, data in, keys out**: commands are `{id, title, keybinding}`; the enum stays as the payload for built-ins; settings rebind by id.
- **File operations are transactions** on the source; the workspace re-keys documents and notifies the index; the Explorer reloads on `fs_epoch`.
- **Every write that is not the user's keystroke lands in the document first** (replace, rename, code actions) and on disk only when the user saves — closed files are the one exception, and those go through `WriteText` with a version check.
- **The LSP crate stays feature-neutral** (`WorkspaceEdit`, `CodeAction`, `CompletionItem` are ours); applying edits is the editor extension's job.
- **Git is data through one function pointer** (`WorkspaceConfig::git`), so the panel is identical on desktop and web.
- **Token, not accounts**; refuse non-loopback binds without one.

## Verified
- Web (Firefox, Playwright, mock provider, fresh fixture per suite): all 23 suites PASS — `milestone1`, `menubar`, `session`, `graph`, `links-sqlite`, `terminal`, `typst`, `lsp`, `ladybug`, `agent`, `search-trace`, `settings`, `rich`, `agent-writes`, `flow`, `wasm-ext`, `phone`, `palette`, `files`, `replace`, `lsp2`, `git`, `auth` (own token-mode server).
- Native: 74 tests pass, 0 failed (3 ignored: live services); clippy and fmt clean; `cargo build -p desktop --features desktop` and `cargo check -p mobile --features mobile` pass.

## What to look at on desktop
1. `Ctrl+Shift+P`, `Ctrl+P` with `:line`; Settings → Keybindings.
2. Right-click a file in the Explorer; drag one into a folder; check `.moonkale/trash/`.
3. Open a `.rs` file: type, `F2`, `Ctrl+.`, `Shift+F12`.
4. Changes tab on this repository: stage, commit, **Graph**.
5. `MOONKALE_TOKEN=test dx serve --port 8080` in `packages/web` → `/login`.

## Deferred to Milestone 8
Entity log (P-33) and collaboration (P-30/P-34), Postgres/Turso (P-19), TypeDB/Helix (P-26), 3D (P-27), browser-side wasm extensions (P-28), JS-free desktop (P-29), user accounts on the server, a CodeMirror merge view for diffs, git push/pull/fetch and branch creation, per-hunk staging, LSP signature help and semantic tokens, OS trash/keychain.
