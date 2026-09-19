---
title: "Milestone 9 — Second Halves: the plan"
description: Finishing what Milestone 8 opened — history you can act on (snapshots, restore), presence with cursors on desktop too, node dragging in 3D, DuckDB as the first data-file engine — and the Android build once the SDK is here.
tags: [milestone, planning]
---
**Goal** (from [[Roadmap]] Phase 9): the follow-ups Milestone 8 named, plus the one driver that needs no server (DuckDB), plus the Android build as soon as Daniel's toolchain installation lands (he is installing the SDK/NDK now, 2026-09-19). Record: [[Milestone 9 - Implementation Log]].

## Starting point (after Milestone 8)
- History: every event is logged, `text_at` replays from a base; nothing restores; the log grows without bound (rewritten whole on each append).
- Presence: web only, active document only; the desktop has no hub client.
- 3D graph: z by layer, orbit camera; node dragging is 2D-only.
- Data: SQLite and LadybugDB embedded; DuckDB (`sources-sql/duckdb.rs`) and Postgres/Turso are stubs. No docker daemon, no Postgres here.
- Android: `mobile` builds, no SDK on the machine (being installed).

## Scope: what "done" means
1. **History you can act on** — `EventKind::Snapshot { live, texts }` written by `EntityLog::compact(keep)` (everything older than the last `keep` events folds into one snapshot; checkpoints are kept); `text_at` starts from the latest snapshot; the History panel gets **Compact** (with the count it would fold) and every text view gets **Restore** (the old text becomes an unsaved edit of the open document — the user saves — with `cause` set to the event restored). Checkpoints show the commit; the history file stays under a size the Settings panel shows.
2. **Presence with cursors, on desktop too** — the code editor reports the cursor line (throttled) → `Member.line`; badges say "at line N" and the editor draws a small marker in the gutter for each other person in the file (CodeMirror gutter markers). The desktop joins a hub given by `MOONKALE_HUB=<http(s)://host:port>` (+ `MOONKALE_TOKEN` for the bearer): a native websocket client speaking the same `PresenceMessage` frames as the web client.
3. **3D node dragging** — dragging a node in 3D moves it in the camera plane (unproject along the view ray at the node's depth); the layout pins it as in 2D. Small but it completes the 3D mode.
4. **DuckDB source** — `sources-sql::duckdb` behind the `duckdb` feature (bundled build, native only): `.duckdb` files open as a source like SQLite (schema, read-only queries, classification), and a **data folder** mode: CSV/Parquet/JSON files in the open folder appear as tables of a virtual DuckDB source (`read_csv_auto` / `read_parquet` views). Desktop and server builds enable it; the web client gets it through `RemoteSource` as with SQLite.
5. **Android build** — when the SDK/NDK/JDK and a device or emulator are available: `dx serve --platform android` on the `mobile` crate, fix what breaks (asset paths, WebView WebGL2, keyboard, storage location), and record the result. Until then this step is blocked and says so.
6. Verify, log, vault.

**Deferred**: Postgres/Turso (still need a server), TypeDB/Helix, JS-free desktop, CRDT, a 3D force layout (z by layer stays the design), time travel that rewrites the working tree from a checkpoint (Restore is per file).

## Architecture decisions
- **Snapshots are events** so the log remains one ordered file; a snapshot is the fold at that point plus the texts the log has bases for. Compaction never drops a checkpoint.
- **Restore never writes to disk**: it puts the old text into the document (or opens the document first) — the usual save path records a new `Content` event whose `cause` is the restored event.
- **Cursor presence is throttled on the client** (≤ 4 updates/s) and carried in the same `Member`; the gutter marker is a CodeMirror gutter extension driven by `setPresence(el, [{line, label}])`.
- **The desktop hub client is `tokio-tungstenite`** speaking JSON text frames — the wire format dioxus's `Websocket<PresenceMessage, PresenceMessage>` uses — with the bearer header when a token is set.
- **DuckDB is the embedded analytics engine**, not a replacement for SQLite: same `Source` shape; data files are views on the fly, so nothing is copied.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | snapshot/compact/restore; History panel buttons; size in Settings | core, ext-api, ui | unit: compact keeps text_at answers and checkpoints; E2E: restore makes the document dirty with the old text |
| 2 | cursor lines, gutter markers, desktop hub client | js/codemirror, editors/code, ext-api, api, desktop | E2E: two browsers see each other's line; native: desktop client unit test against the hub |
| 3 | 3D drag | graph-render | unit: unproject round trip; E2E (Chromium): drag moves a node in 3D |
| 4 | DuckDB source + data-folder views | sources-sql, desktop, api, ui | unit: query a `.duckdb` and a CSV; E2E: CSV appears as a table |
| 5 | Android | mobile | on the device/emulator, when available |
| 6 | verify, log, vault | | |

## Risks
| risk | mitigation |
|---|---|
| DuckDB bundled build time (C++) | measured before committing to it; the feature stays off for the web client and optional elsewhere |
| Websocket framing of dioxus server-fn sockets | verified against the running hub with the native client's own test before wiring the desktop |
| Compaction losing a base | a snapshot carries every text the log could replay; a test compares `text_at` before and after |
| Android tooling arrives late | step 5 is last and independent; the log records "blocked" honestly if it does not |
