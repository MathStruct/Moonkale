---
title: "Milestone 1 — Implementation Log"
description: What was actually built for the walking skeleton, what deviated from the plan and why, and the problems hit on the way.
tags: [milestone, log]
---
The plan was [[Milestone 1 - Walking Skeleton]]. This is what happened, step by step, on 2026-09-17. Per-crate notes sit next to the code (`packages/<crate>/<crate>.md`; links below go to GitHub so they work from the published site — in Obsidian, open them from the file explorer).

> [!success] Result
> The walking skeleton works end to end on the **web** build (server-side folder) and compiles for **desktop** and **mobile** with the in-process folder source. An automated Playwright run ([`packages/web/tests/e2e/milestone1.mjs`](https://github.com/MathStruct/Moonkale/blob/master/packages/web/tests/e2e/milestone1.mjs)) covers: open folder → lazy tree → open file → edit → dirty marker → Ctrl+S → file on disk changed → external edit → save conflict → reload → second file → drag tab into a split → editor survives the remount. 12 Rust tests pass; clippy and fmt are clean. The desktop build was **not run** (no display on the dev box) — that is the one manual check left.

## Steps as executed

| # | step | outcome | note |
|---|---|---|---|
| 1 | `moonkale-core` | ✅ 7 unit tests | [core.md](https://github.com/MathStruct/Moonkale/blob/master/packages/core/core.md) |
| 2 | `moonkale-project-fs` | ✅ 5 integration tests | [project-fs.md](https://github.com/MathStruct/Moonkale/blob/master/packages/project-fs/project-fs.md) |
| 3 | `moonkale-sources` registry | ✅ | [sources.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources/sources.md) |
| 4 | `api` server functions + `RemoteSource` | ✅ client / server / wasm all compile | [api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/api/api.md) |
| 5 | `moonkale-ext-api` | ✅ | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md) |
| 6 | `packages/js/codemirror` bundle | ✅ 284 kB, committed at `editors/code/assets/codemirror.js` | `PROTOCOL.md` |
| 7 | `moonkale-editor-code` | ✅ | [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md) |
| 8 | `ui` shell + explorer + platform wiring | ✅ | [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 9 | verify | ✅ web E2E ×2 (before/after a fix); desktop compile only | below |
| 10 | document | this note + crate notes + vault updates | |

## Deviations from the plan

1. **`RemoteSource` lives in `api`, not `sources`** (planned in step 3, confirmed). `api` must own the registry *and* the client stub must call `api`'s server functions; one crate avoids the cycle. [[Data Sources]] and [[Project Structure]] updated.
2. **JS ↔ Rust transport is `dioxus.send()` / `dioxus.recv()` inside one `document::eval` per editor**, not `CustomEvent`s. It is the documented Dioxus 0.7 channel, works identically in the browser and every webview, and needs no global listener. The initial text is *sent* over the channel rather than formatted into the script, so no escaping and no size limit in the script string. [[JS Interop Boundary]] updated.
3. **`Document` lives in `ext-api`, not in the editor crate.** The workspace owns open documents so they survive panel remounts; the editor is a view. This is [[ADR-0008 Rust owns the document, JS is a view]] taken literally, and the E2E drag-to-split step proves it.
4. **Whole-document changes.** JS reports the full text on every keystroke and the save sends one whole-document `Splice`. `TextPatch` already accepts real splices; the producer will change, the consumer will not. Fine at Milestone-1 file sizes; a known cost.
5. **Platform function pointers instead of `Rc<dyn Fn>`.** `OpenFolder` is a plain `fn` so it can be a component prop; wrapped in `ShellConfig` with a documented always-equal `PartialEq` because fn-pointer comparison is meaningless (clippy said so).
6. **The `ui` crate lost the template's `Hero`/`Echo`** and the Prompt-1 dummy `EditorWorkbench`. `Navbar` stays because the routers use it.
7. **Explorer is in `ui`**, as planned, but written as an `Extension` — it uses only `Workspace`, exactly like the editor.

## Problems hit (→ [[Problem Log]])

- **P-034 Hydration mismatch from `cfg!` in markup.** The explorer's input placeholder used `cfg!(target_arch = "wasm32")` to word the hint per platform. Fullstack renders the page on the *server* first (native), hydration keeps the server's markup, so the web build showed the native text. Rule: rendered output must not depend on compile-time platform. Fixed with one neutral string. Found by looking at a screenshot, not by any check — a `dx` hydration warning would have helped.
- **P-035 `.gitignore` ignored outside a git repo.** The `ignore` crate honours `.gitignore` only inside a repository by default; the test fixture wasn't one and `target/` showed up. `require_git(false)` — an editor should behave the same before and after `git init`.
- **P-036 Server-function name clash.** `#[post] async fn query(source, query: Query)` failed to compile: the macro-generated client stub calls `query(...)` and the parameter shadowed it. Renamed to `query_source` / `fetch_text_from` / `apply_to`. Rule: server-function names must not collide with their own parameter names or with `core` type names in scope.
- **P-037 `pkill -f "dx serve"` kills the shell that runs it.** Twice. Use `pkill -x dx`. (Tooling, but it cost two runs.)

## Decisions worth keeping

- **Version = hash(mtime, len)** for files. Cheap, correct enough for optimistic concurrency, and the conflict path is tested. Content hashes come with the index.
- **Atomic writes** via sibling temp file + `rename`. Readers never see a torn file; the temp name is `<file>.<ext>.moonkale-tmp`.
- **`async-trait` with `?Send` on wasm32** via two `cfg_attr` lines, re-exported from `core` so implementors don't add the dependency. Both `cargo check` targets run on every step.
- **`MOONKALE_ROOT` confines `open_folder` on the server**; default is the server's cwd. No auth. `dx serve` is a local dev tool until [[Platform Matrix]]'s security list is done.
- **Extensions are constructed by a `fn() -> Vec<Box<dyn Extension>>`** the platform passes in (`ui::default_extensions`). No `inventory` magic yet; explicit is fine for two extensions.

## Addendum (same day): VS Code-style frame

After the milestone: the template's Home/Blog router and navbar were replaced by `ui::Frame` + `ui::TitleBar` (File/Edit/View/Help, centered title, and on desktop minimize/maximize/close on the same bar with drag and edge-resize on an undecorated window). A small **command bus** on `Workspace` connects menus, keybindings and buttons to the active editor and the shell. The desktop crate gained a **native folder dialog** (`rfd`, xdg-portal backend) — the answer to "why doesn't Open show the Linux dialog": M1 shipped a text field by design. Verified on web by the E2E script plus `packages/web/tests/e2e/menubar.mjs`; the desktop window chrome itself is untested here (no display). P-039, P-040.

## Addendum: multi-window session

*View → New Window* (desktop: a second `dioxus::desktop` window in the same process; web: a new tab) joins a **session bus** (`ext-api::session`): peers exchange `Hello`/`SourceOpened` so a new window shows the same folder, and an editor's path label is an HTML5 drag handle — drop it on another window to **move** the document there (`DragStarted` → drop overlay → `Moved` → origin closes). Transports: in-process channels + a process-wide `SourceRegistry` on desktop, `BroadcastChannel` on web. Verified on web with `packages/web/tests/e2e/session.mjs` (two tabs; the drag is simulated by dispatching the HTML5 events — the OS-level drag between two browser windows is a manual check, as is the desktop). Problems: P-041, P-042, P-043. Design continuation: [[Collaboration]].

## What Milestone 1 does *not* do (on purpose)

File watching · syntax highlighting · LSP · search · more than one source at a time · layout persistence · native folder picker · auth · mobile layout adaptation · streaming/capping large files over server functions. All listed in the plan as out of scope; all still in [[Problem Ranking]].

## How to run it

See `packages/web/tests/e2e/README.md` (web, automated) and [[Development]] (desktop: `cd packages/desktop && dx serve --platform desktop`, type a path, Open).
