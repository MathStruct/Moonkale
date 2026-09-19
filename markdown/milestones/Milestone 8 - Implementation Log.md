---
title: "Milestone 8 — Implementation Log"
description: What was built for "Research" — the entity log, presence, the 3D graph and wasm extensions running in the browser — what deviated, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 8 - Research]].

> [!success] Done (2026-09-19)
> All four research bets that could be built here are built and verified on the web build; desktop shares the code (presence needs a hub, so it is web-only for now). **Entity log** ([[ADR-0012 Two histories]] → accepted): `core::graph::history` records every create, rename, delete, content patch (user or agent) and git checkpoint in `.moonkale/history.jsonl`; a **History** panel lists them, filters to the active file and shows any file's text as it was after any event. **Presence**: a room per folder on the server; other people's initials on the status bar, on the tabs of the files they have open and in the Explorer. **3D graph**: a perspective, orbiting camera over one plane per node kind, with depth-tested, fogged edges — and the first automated test that exercises the wgpu renderer itself (Chromium + SwiftShader). **wasm extensions in the browser**: the same JSON-ABI core module runs in a Worker with a `SharedArrayBuffer` mailbox; host calls are answered by the client's own sources; the server path remains the fallback. Three new E2E suites (`history`, `presence`, `graph3d`) plus an extended `wasm-ext`; 26 suites in all; 78 native tests; clippy/fmt clean.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `core::graph::history`: `EventId` (time-ordered), `Event { id, at, actor, key, cause, kind }`, `EventKind::{Add, Remove, Rename, Content{patch, base}, Checkpoint}`, `EntityLog::{append (ordered), merge, for_node (rename chains), fold, text_at, to_jsonl/from_jsonl}`; `settings.user_name`; `Workspace::{record, record_as, record_event, load_history}`; appends from save/create/create_dir/rename/delete; agent edits attributed via `pending_actor`; checkpoints from the Changes panel; History panel (`ui::history`) | ✅ 3 unit tests, E2E `history.mjs` | [core.md](https://github.com/MathStruct/Moonkale/blob/master/packages/core/core.md), [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md), [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 2 | `ext-api::presence` (`Member`, `PresenceMessage`, `PresenceLink`, `JoinPresence`); `api::presence` hub (rooms, broadcast, leave on disconnect) + `RemotePresence` client; `Workspace::{join_presence, publish_presence, others}`; frame publishes on active/name change; badges in status bar, tabs, Explorer | ✅ 1 unit test, E2E `presence.mjs` (two browser contexts) | [api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/api/api.md) |
| 3 | `graph-render`: `Node.z` by kind layer (`layer_z`), `Camera` 3D fields + `view_proj`/`project`/`orbit`/3D `pan`/`zoom`/`fit`/`hit`, uniform block with `mat4`, shaders with a `project()` branch, screen-space node radius attenuated by depth, fog, depth buffer (`Depth24Plus`), `set_mode`, right/Shift-drag orbit; Graph panel **3D** toggle (`data-mode`) | ✅ 2 unit tests, E2E `graph3d.mjs` | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph-render/graph-render.md), [graph.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph/graph.md) |
| 4 | `js/wasm-host` (Worker + SAB mailbox → `ui/assets/wasm_host.js`); `api::module_bytes` at `/api/ext/module/{id}`; COOP/COEP headers (`MOONKALE_ISOLATE=0` disables); `Workspace::run_wasm_in_browser` + `answer_host_call` (same permission check as the host); `WorkspaceConfig::wasm_module_url` (web) | ✅ E2E `wasm-ext.mjs` (isolated page, zero server runs, works with `/api/ext/run` blocked) | [ext-host.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-host/ext-host.md), [js/README.md](https://github.com/MathStruct/Moonkale/blob/master/packages/js/README.md) |
| 5 | verify, log, vault, ADR-0012 accepted | ✅ | this note |

## What the user sees
- **History** tab (side tile): every event with who/when/what; tick *active file* to see one file's story; *text* opens the file as it was after that event in the main tile. Settings → You → Name sets the actor (and the presence name).
- Other people in the same folder appear as initials badges: 👥 in the status bar (hover for names and what they look at), on tabs of files they have open, next to files in the Explorer.
- Graph tab → **3D**: one plane per kind (directories below files below symbols; commits on top); drag pans, right-drag or Shift-drag orbits, wheel dollies, **Fit** frames the whole thing; hover and double-click work as in 2D.
- wasm extensions on web run in the page when the browser allows it (`crossOriginIsolated`), so their host calls never leave the client; nothing to configure.

## Deviations from the plan
1. **Content events carry a `base`** (the text before the first patch) for files that existed before the log did, so `text_at` works for every file, not only those created after the log started; `Add` events of new text files carry the initial text. Events also carry the node's `key` for display.
2. **The log file is rewritten whole on every event** — simplest correct thing at this size; compaction/snapshots are the next step when a workspace passes ~10k events.
3. **Agent attribution happens at save time**: `editor.replace` marks the document (`pending_actor`), the save records `agent:<model> (saved by <user>)`. One event per save, however many agent edits preceded it.
4. **Presence carries the active document but not the cursor line** — a cursor stream would need CodeMirror selection events and would flood the hub for little value; documented as a follow-up.
5. **Desktop has no hub** (`presence: None`): its own windows share a process bus already; joining a remote hub from desktop (`MOONKALE_HUB`) is the next step, not built.
6. **3D uses the 2D layout with z by layer**, as planned; the depth buffer made the plan's "sort edges per frame" unnecessary. Node dragging is 2D-only; in 3D the drag pans.
7. **The Chromium/SwiftShader E2E is the first renderer test**: Firefox headless cannot create WebGL at all (P-082), Chromium with `--use-angle=swiftshader` can; the suite reads the label overlay (the GL canvas cannot be read back after present) and compares screenshots across an orbit.
8. **The browser runtime is used whenever the page is isolated** rather than behind a flag; `MOONKALE_ISOLATE=0` turns the headers off if a deployment needs cross-origin resources. The Worker is created per run (modules are small; instantiation is milliseconds) and terminated after.
9. **Not built, as announced**: Postgres/Turso, TypeDB/Helix, Android, JS-free desktop, CRDT text merge, 3D force layout.

## Problems hit (→ [[Problem Log]])
- **P-082 Firefox headless has no WebGL**: every renderer path was untested until now; Chromium headless with SwiftShader (`--use-angle=swiftshader --enable-unsafe-swiftshader`) provides WebGL2, so `graph3d.mjs` runs in Chromium while everything else stays on Firefox.
- **P-083 A settings label selector matched the wrong field**: `label:has-text('Name')` also matched "Secret name"; suites use `label:text-is('Name')`.
- **P-084 `wgpu` 30 depth state fields are `Option`s** (`depth_write_enabled: Some(true)`, `depth_compare: Some(..)`) — noted for the next pipeline.
- **P-070 again**: the renderer wasm is rebuilt by `build.sh`, the wasm-host bundle by `npm run build`; both need a `.rs` touch and a dx restart.

## Decisions worth keeping
- **The entity log is a file in the workspace**, JSONL, one event per line, rewritten whole; keys and bases make every event self-describing.
- **`text_at` is a replay**, not a snapshot store; the log stays small and the History panel stays honest about what it can show.
- **Presence is a room per folder id with in-memory state**; the message enum is shared between hub and client like every other relay.
- **3D is a camera, not a second renderer**: one uniform block, one shader with a `project()` branch, a depth buffer.
- **The JSON ABI runs unchanged in the browser**; the Worker + `SharedArrayBuffer` mailbox is the only browser-specific piece, and the host side is the same three calls with the same permission check.

## Verified
- Web: 26 suites — `milestone1` (own root) and `menubar`, `session`, `graph`, `links-sqlite`, `terminal`, `typst`, `lsp`, `ladybug`, `agent`, `search-trace`, `settings`, `rich`, `agent-writes`, `flow`, `wasm-ext` (extended), `phone`, `palette`, `files`, `replace`, `lsp2`, `git`, `auth`, `history`, `presence`, `graph3d` (Chromium) — all PASS in one batch.
- Native: 78 tests pass, 0 failed (2 ignored: live services); clippy and fmt clean; desktop builds; `mobile` checks.

## What to look at on desktop
1. History tab after a few edits and a commit; *text* on an older event.
2. Graph → **3D** on this repository's index (right-drag to orbit).
3. Settings → You → Name — it shows in History as the actor.

## Deferred to Milestone 9
Log compaction and snapshots; checkpoints that restore a state (time travel across git); presence cursors and a desktop hub client; 3D force layout (octree Barnes–Hut) and node dragging in 3D; per-extension Worker reuse and a browser-side manifest cache; Postgres/Turso, TypeDB/Helix, Android, JS-free desktop, CRDT.
