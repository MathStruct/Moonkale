---
title: "Milestone 9 — Implementation Log"
description: What was built for "Second Halves" — snapshots and restore in the history, presence cursors and a desktop hub client, 3D node dragging, DuckDB and data-file tables — and the first Android build running on a phone.
tags: [milestone, log]
---
Plan: [[Milestone 9 - Second Halves]].

> [!success] All six steps done (2026-09-19) — including the first Android build on a real phone
> **History you can act on**: the log compacts into a `Snapshot` event (everything but the last 200 events, checkpoints kept) and any earlier text can be **restored** as an unsaved edit whose save records the restored event as its `cause`. **Presence with cursors**: the editor reports the cursor line (throttled), other people's initials appear in a gutter next to their line and the badges say `file:line`; the **desktop joins a hub** with `MOONKALE_HUB` (+ `MOONKALE_TOKEN`), verified against the running hub with a native websocket test. **3D dragging**: a node dragged in 3D moves in its own depth plane (`Camera::unproject`, round-trip tested). **DuckDB**: `.duckdb` files open like SQLite, and a CSV/TSV/Parquet file opens its folder as a database of views — SQL joins across files work from the table editor. One new suite (`duckdb`), three extended (`history`, `presence`, `graph3d`); 27 suites in all; 82 native tests; clippy/fmt clean. **Android**: with the NDK pulled and Daniel's Galaxy S10e on USB, the release arm64 APK builds, installs and runs — phone shell, Explorer, editor with the soft keyboard, save → file + `history.jsonl`, settings persisted, graph on WebGL2 — after five Android-only fixes (P-087–P-091), one of which (`Stylesheet`) touches every panel.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `EventKind::Snapshot { live, texts, folded }`, `EntityLog::compact(keep, at, actor)` (folds older events, keeps checkpoints, snapshot at the boundary), `text_at` starts from the latest snapshot; `Workspace::{compact_history, restore_text_at}` + `pending_cause` → `Event.cause`; History panel **Compact (n)** and **Restore** in the text view, size in KB | ✅ 1 unit test (answers and checkpoints survive compaction, JSONL round trip), E2E `history.mjs` (restore → dirty editor, disk untouched, save carries `cause`) | [core.md](https://github.com/MathStruct/Moonkale/blob/master/packages/core/core.md), [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 2 | bundle: `onCursor` (250 ms throttle) + `setPresence` gutter (`cm-presence-gutter`); `BackendEvent::Cursor`, `Workspace::{cursor_line, set_cursor_line}`, `Member.line`; `desktop/src/presence.rs` (`tokio-tungstenite`, JSON binary frames, bearer) | ✅ E2E `presence.mjs` (gutter mark + `README.md:3` badge), native test `desktop/tests/hub.rs` (ignored unless `MOONKALE_HUB`) passed against the dev hub | [editor-code.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/code/editor-code.md), [desktop.md](https://github.com/MathStruct/Moonkale/blob/master/packages/desktop/desktop.md) |
| 3 | `Camera::{basis, unproject}`; `Drag::Node` in 3D moves the node at its depth | ✅ unit test (project ∘ unproject), E2E `graph3d.mjs` (a label moves after a drag) | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph-render/graph-render.md) |
| 4 | `sources-sql::duckdb` (feature `duckdb`, bundled): `DuckDbSource::open` (read-only) and `open_data_folder` (views over `read_csv_auto` / `read_parquet`, ≤ 200 files, sanitised names), schema via `information_schema`, read-only text queries, value mapping; `is_duckdb_path` / `is_data_path`; desktop + server open dispatch; Explorer marks data files | ✅ 2 unit tests, E2E `duckdb.mjs` | [sources-sql.md](https://github.com/MathStruct/Moonkale/blob/master/packages/sources-sql/sources-sql.md) |
| 5 | Android: `app_folder()` seeds a vault in the app's files dir, `settings.json` next to it, `reopen_last_folder`; `moonkale_ext_api::Stylesheet` + `Workspace::assets_epoch` (P-087); absolute asset URLs in the renderer eval (P-088); `prefer: "gl"` on Android WebViews → `Renderer::new(.., gl_only)` (P-089); non-sRGB surface format (P-090); re-fit on resize while untouched (P-091) | ✅ on the Galaxy S10e (SM-G970F, LineageOS, Android 13): install, launch, Explorer, edit, save, history, settings, graph — screenshots via `adb exec-out screencap`, DOM checks via the WebView's DevTools socket | [mobile/README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/mobile/README.md) |
| 6 | verify, log, vault | ✅ | this note |

## What the user sees
- History: **Compact (n)** in the head folds old events into a snapshot (the KB count drops); **Restore** in a "text at" view puts that text into the editor as an unsaved edit — save to keep it, and the History shows the save with the restored event as its cause.
- Presence: someone else's initials next to the line they are on; the status-bar badge reads `Name · file:line`. On desktop, `MOONKALE_HUB=http://host:8080 MOONKALE_TOKEN=… dx serve --platform desktop` joins the same rooms as the web users.
- Graph → 3D: drag a node; it stays in its plane and follows the pointer.
- Click a `.csv`, `.tsv` or `.parquet` in the Explorer: its folder appears as a database (`data/ (data files)`) with one view per file; `.duckdb` files open directly. SQL across files works in the table editor; writes are refused.
- On the phone: the app opens its own vault (`Home.md`, `notes/First note.md`, `Ideas.md`, `main.rs`) in the bottom-bar shell; Files, Editor (soft keyboard, dirty dot, Save), Graph (fitted, WebGL2), Settings and Agent are the same panels as on desktop. The vault lives in the app's private storage for now.

## Deviations from the plan
1. **Compaction keeps the last 200 events** (a constant in the History panel) rather than a size budget; the snapshot's `texts` hold every text the log could replay, so `text_at` before the boundary answers exactly as before — the test asserts it.
2. **Restore is the whole file**, not a hunk; the diff is the user's to review in the editor.
3. **Cursor presence is line-only** (no column, no selection ranges) and throttled in the bundle, not the hub.
4. **The desktop hub client speaks the dioxus websocket format directly** (JSON in binary frames) rather than reusing the server-function client; `ws://` and `wss://` both work, TLS via the system roots is not wired (behind a proxy on the LAN this is `ws://`).
5. **DuckDB data folders are non-recursive** and capped at 200 files; JSON is left to the text editor (too often configuration, not data).
6. **DuckDB adds ~3 minutes to a clean release build** (measured 3 m 14 s for the bundled library alone); it is a feature on `sources-sql`, on for desktop and the server, never for the web client.
7. **Android is a release build only**: the debug APK (x86_64 by default, ~5× larger) did not fit the phone's 1.4 GB free space (`INSTALL_FAILED_INSUFFICIENT_STORAGE`) and the wrong ABI (`INSTALL_FAILED_NO_MATCHING_ABIS`); `dx build --release --platform android --features mobile --target aarch64-linux-android` gives a 23 MB APK that installs. `dx serve` hot-reload on the device was not used.
8. **Storage is app-private** (`/data/data/io.github.mathstruct.moonkale/files/vault`); opening a folder the user chooses needs the Storage Access Framework — Milestone 10.
9. **The Android graph runs on WebGL2, not WebGPU**: the WebView's WebGPU never completes device creation (P-089), so the panel asks for `gl` on Android WebViews. The `prefer` parameter is a hint, not a policy: other platforms still try WebGPU first.

## Problems hit (→ [[Problem Log]])
- **P-085 rustfmt-shaped anchors**: three multi-edit scripts failed half-way because the file had been reformatted since the last read; each time the fix was to re-read the exact text first. Not a product problem, but the third time it cost real minutes — noted as a working rule.
- **P-086 Compaction and rename chains**: `for_node` walks renames backwards through events; after compaction the pre-boundary renames are gone, so `text_at` starts from the snapshot's text keyed by the *current* id — which is what the fold produced. Correct, but the History panel's key lookup for very old events now relies on `Event.key`.
- **P-087 Android drops head elements from the first render**: three panels came up unstyled. `moonkale_ext_api::Stylesheet` inserts the link itself (idempotent eval) and re-checks when `assets_epoch` bumps — all fourteen `document::Stylesheet` sites now use it.
- **P-088 `import()` has no base URL in a WebView eval**: resolve against `document.baseURI` first.
- **P-089 WebGPU hangs on the Android WebView**: adapter yes, device never; `prefer: "gl"` for `Android … wv` user agents.
- **P-090 sRGB surface on GL**: grey background, pale nodes; pick a non-sRGB format.
- **P-091 Auto-fit before the canvas had a size**: every node in the middle of the screen; `resize()` re-fits while the view is untouched.
- Debug APK: wrong ABI and too large for the phone; release + `--target aarch64-linux-android`.

## Decisions worth keeping
- **Snapshots are events at the boundary**, checkpoints survive them, and replay is snapshot-then-patches.
- **Restore goes through the document**, never the disk, with provenance on the save.
- **The hub's wire format is the dioxus typed-websocket format**; any native client can join with a websocket library.
- **DuckDB is the data-file engine**: nothing is imported, views are created at open time.
- **Panels own their stylesheets** through `moonkale_ext_api::Stylesheet`, never `document::Stylesheet` directly — the one place that knows how each platform gets a `<link>` into the head.
- **Android is verified on a device, not an emulator**: no AVD was created; `adb` + the WebView DevTools socket (`adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`) give screenshots and DOM queries without touching the phone.

## Verified
- Web: 27 suites — the 25 of Milestone 8 (with `history`, `presence`, `graph3d` extended) plus `duckdb` and `milestone1` — all PASS in one batch.
- Native: 82 tests pass, 0 failed (3 ignored: live services and the hub test); clippy and fmt clean; desktop builds (with DuckDB).
- Android: release APK on the Galaxy S10e — stylesheets present (`main,shell,titlebar,explorer,panel,links,xterm,terminal,agent,git`), vault reopened on launch, Graph `5 nodes · 4 edges · gl` fitted on first open, typed text saved to `files/vault/Home.md` with a `content` event in `.moonkale/history.jsonl` and the folder in `settings.json` → `recent_folders`.

## What to look at on desktop
1. History → Compact after a day of edits; Restore an old version of a note.
2. `MOONKALE_HUB=http://127.0.0.1:8080 dx serve --platform desktop` next to a web session: the web users' initials appear in the desktop window.
3. Drop a CSV into a folder and click it.

## Android — how it was done (2026-09-19)
Toolchain: JDK 17, SDK cmdline-tools, platform-tools 37, build-tools 36, platform android-37, NDK 29.0.14206865 (`sdkmanager --sdk_root=$HOME/Android/Sdk "ndk;29.0.14206865"`), Rust targets `aarch64-linux-android` + `x86_64-linux-android`. Device: Samsung SM-G970F, LineageOS, Android 13, arm64-v8a, WebView 125, USB debugging. Recipe in [[Development]] (Android section) and `packages/mobile/README.md`.

What broke, in order: debug APK ABI → target flag; debug APK size → release; three panels unstyled → P-087; renderer import → P-088; renderer hang → P-089; grey canvas → P-090; graph in one spot → P-091. Each fix was a rebuild (≈ 8 s incremental, ≈ 4 min from clean), `adb install -r`, launch, `screencap`.

Not done on the phone: opening user folders (SAF), the terminal (no PTY on Android — the panel shows its unavailable state), LSP (no rust-analyzer), presence (no hub configured), landscape layout.

## Deferred to Milestone 10
Android storage via SAF and a signed release build, snapshot-aware `for_node` for keys, selection-range presence, TLS for the desktop hub client, recursive data folders and JSON tables, Postgres/Turso, TypeDB/Helix, JS-free desktop, CRDT.
