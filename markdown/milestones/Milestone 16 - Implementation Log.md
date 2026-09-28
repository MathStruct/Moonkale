---
title: "Milestone 16 — Implementation Log"
description: What was built for "Sources follow the disk" — the Sources panel follows changes made outside Moonkale (↻ for sources that are not watched), the graph's Local view lays itself out and gives Whole back unchanged, the rich editor writes no <br /> and cannot hang at "Loading…", and tabs can be dragged by touch.
tags: [milestone, log, explorer, sources, graph, markdown, android]
---
Plan: [[Milestone 16 - Sources Follow the Disk]]. From [[Prompt26]].

> [!success] Done 2026-09-28: 4 new Rust tests, 4 JavaScript tests, 42 browser suites (3 new)
> **Sources follow the disk.** A file created, edited or deleted by another program (an editor, `git checkout`, an agent in a terminal) shows up in the Sources tree and the index without a click, on the desktop and in every client of a server: the browser, the phone, a remote SSH folder. Hidden and `.gitignore`d paths are not watched at all. A source that is not watched (a database) has a **↻** button, and every source has *Refresh* in its context menu. The side panel is called **Sources**. **Graph → Local** gets its own compact layout; **Whole** comes back with the same camera and positions. The rich editor writes **no `<br />`** for empty lines. Its start is bounded: a failure shows the reason and **Retry** instead of "Loading rich editor…". **Tabs drag by touch**: a long press picks a tab up and the finger docks it.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `core`: `Changes { seq, paths, reset }`, `Source::changes_since(since)` (default `None` = not watched) | ✅ | `source/event.rs` was a stub since Milestone 1 |
| 2 | `project-fs`: `FolderWatch` (`notify` 8; one non-recursive watch per visible directory, new directories added, a bounded log of relative paths, long poll with a 120 ms settle); `FolderSource::changes_since` starts it on first use | ✅ `tests/watch.rs`: 3 tests | [project-fs.md](https://github.com/MathStruct/Moonkale/blob/master/packages/project-fs/project-fs.md) |
| 3 | `api`: `/api/sources/changes` (a long poll of ≤ 25 s); `RemoteSource::changes_since` forwards to it | ✅ `cargo check -p api --features server`, web wasm build | one request per source per client |
| 4 | `ext-api`: `Workspace::follow_sources` (one root-owned loop per source; resolve paths via the `path` dialect, index `refresh`, `graph_epoch` + `fs_epoch`; `reset` → from the root; back-off on errors), `watched`, `refresh_source`; the frame calls it whenever `sources` changes | ✅ `tests/follow.rs` (VirtualDom): watched vs not, an edit and a deletion reach the index, reset, ↻ on a database, the loop ends when the folder closes | |
| 5 | Sources panel: ↻ on unwatched sources, *Refresh* in the root's context menu; "Explorer"/"Files" → **Sources** (panel, activity, palette `View: Show Sources`); ids stay `explorer` so saved layouts keep working | ✅ `watch.mjs`; 7 suites updated for the new label | |
| 6 | Graph: `GraphView::{set_graph_fresh, stash, unstash}`; the panel stashes `whole` entering Local, lays a new centre out fresh, unstashes leaving Local | ✅ `graph-local.mjs`: temperature 25.8 (fresh, not 3.0 warm), camera and positions identical after Whole | P-139; renderer rebuilt (`build.sh`) |
| 7 | Milkdown: `src/clean.ts` drops `<br />`-only lines and collapses blank runs outside fences, applied to every report; bundle rebuilt | ✅ `npm test` (4 cases); `rich.mjs`: `<br />` file opens clean, save writes `"# Gaps\n\nfirst\n\nsecond\n\nthird\n"` | P-140 |
| 8 | Rich mount: bounded waits (element, bundle inserted if absent, `crepe.create()`), `RichEvent::Failed`, reason + **Retry** | ✅ normal start covered by `rich.mjs`, `wiki.mjs`; the failure path is not provoked by a test | P-141 |
| 9 | Touch drag: `ui/src/touch_drag.rs`, a long press (350 ms) → synthetic `dragstart`/`dragenter`/`dragover`/`drop`/`dragend` at the finger; CSS against selection and the callout | ✅ `touch-drag.mjs` at 420 px: the phone tile splits; swipe and tap unchanged. **On the S10e** (release APK, updated in place, vault kept): a long press + drag docked Graph below the editor | no fork of `dioxus-workbench`; P-142 |
| 10 | Verify, log, vault | ✅ clippy clean on the changed crates, 42/42 suites; P-133–P-141 (the CI problems of the 26th included); Home, README, Getting Started | |

## Deviations from the plan
1. **Item 6 of the prompt is not done.** "Linux: dragging between two windows might not follow the cursor but move faster sometimes" — the vertical middle divider, seen once and not reproducible (Daniel, 2026-09-28). The splitter's arithmetic is scale-consistent, the display runs at scale 1 and nothing sets a CSS zoom; a stale measurement after a layout change is the only hypothesis. Recorded as P-143, nothing changed.
2. **The intermittent "Loading rich editor…" was not reproduced.** The fix removes every unbounded wait, so the panel can no longer hang silently, and it reports the cause. If it happens again, the message says which wait failed.
3. **Touch drag first failed on the phone although the emulation passed** (P-142): the Android WebView starts its own native drag of a `draggable` element on a long press and cancels the touches. A held tab is now not draggable; verified on the S10e, and the three touch/phone suites pass again.
4. **Open documents are not reloaded when their file changes on disk**, as planned. The tree and the index follow; an open, unmodified editor still shows the old text until *Reload*.

## Added after the plan
- **Android system bars in the app's colour** (Daniel, 2026-09-28): dx's template theme is AppCompat *Light*, which left a grey strip above the app. `build-android.sh` now writes a dark `AppTheme` with status bar, navigation bar and window background `#0b0d12` (the title bar's colour; no light flash at start either). Verified on the S10e. Android 15's edge-to-edge enforcement for apps targeting API 35 would ignore the bar colours; its opt-out attribute needs compileSdk 35 and dx's template compiles against 34, so that case is open (no Android 15 device).

## Problems hit (→ [[Problem Log]])
- **P-139** the Local graph kept the Whole layout's positions.
- **P-140** Milkdown's `<br />` for empty paragraphs.
- **P-141** unbounded waits behind "Loading rich editor…".
- **P-142** the Android WebView's native long-press drag cancelled the touch drag.
- **P-143** (observed once) the divider outran the mouse on Linux.
- Writing the suites: the rail entry toggles, squeezed tab titles, a swipe scrolls the tab strip (E2E README).
