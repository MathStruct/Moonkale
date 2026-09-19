---
title: "Milestone 8 — Research: the plan"
description: The four research bets that can be built and verified on this machine — the entity log, presence, a 3D graph, and wasm extensions running in the browser — and the ones that cannot (yet).
tags: [milestone, planning]
---
**Goal** (from [[Roadmap]] Phase 8): the long-term direction items. Not every one is buildable here: no Android SDK, no Postgres/TypeDB/Helix servers (and no docker daemon), and a JS-free desktop is a multi-milestone rewrite. This milestone takes the four that **can be built and verified now** and records the rest with what they wait for. Record: [[Milestone 8 - Implementation Log]].

## Starting point (after Milestone 7)
- History: git only ([[ADR-0012 Two histories]] is still *proposed*); `core::graph::history` is a doc stub; nothing records structural changes (create/rename/delete), agent edits or database writes.
- Collaboration: the session bus joins windows of one browser/process; nothing crosses machines; no notion of a user (`WindowId` only).
- Graph: 2D only; `NodeKind` decides colour, not depth.
- wasm extensions: JSON ABI over core modules, run by wasmtime on desktop and the server; the browser only relays.
- Machine: Firefox headless supports `SharedArrayBuffer` when the page is cross-origin isolated; `julia`, `git`, `rust-analyzer` present.

## Scope: what "done" means
1. **Entity log** (P-33; ADR-0012 → *accepted*) — `core::graph::history`: `Event { id (uuid v7-ish: time + random), at, actor, cause, kind }` with `EventKind::{Add(Node), Remove(NodeId), Rename{from,to}, Content{node, patch, chars_after}, Checkpoint{commit}}`; `EntityLog` append-only, `fold` to a state (`live` nodes, tombstones), `text_at(node, event)` replays content patches; persisted as `.moonkale/history.jsonl` in the workspace (through the folder source, like settings). Every workspace write appends: save (Content), create/new dir (Add), rename (Rename), delete (Remove), agent edits (Content with `actor = agent`), git commit from the Changes panel (Checkpoint ↔ commit hash). A **History** panel lists events (newest first, filter to the active document), shows a node's text as it was after any event, and marks checkpoints. `actor` comes from a new `settings.user.name` (default `$USER`).
2. **Presence** (P-34) — a server hub (`/api/presence?room=<folder id>`): members `{window, user, active document, line}`, full member list broadcast on every change, removal on disconnect. Web clients join when a folder opens; the status bar shows who is here, tabs of documents someone else has open carry their initials, the Explorer marks their files. Desktop joins the same hub when `MOONKALE_HUB=<ws url>` is set (documented; the desktop's own multi-window session stays local).
3. **3D graph** (P-27) — the renderer gets a `3D` mode: z from a *layer* per node kind (directories / files / symbols / pages / commits) plus a small force spread, a perspective camera (orbit with the right button or Shift-drag, zoom, fit), depth-sorted edges with fog, labels and hit-testing in projected space. Toggle in the Graph panel toolbar; the 2D path is untouched.
4. **wasm extensions in the browser** (P-28) — the same JSON-ABI core module runs client-side: a Worker instantiates the module (bytes served by the server from the extension directory), host calls (`list_sources`, `query`, `fetch_text`) go to the main thread over a `SharedArrayBuffer` + `Atomics.wait` mailbox and are answered by the client's `Workspace` (remote sources), permissions checked on the client as on the host. Needs cross-origin isolation (`COOP`/`COEP` headers on the server). Web `wasm_run` prefers the browser runtime and falls back to the server. Verified with `wordcount` in Firefox.
5. Web parity (history and presence are web-first; 3D and browser wasm are web features that also run in the desktop webview); E2E; documentation; ADR-0012 marked accepted with the implemented shape.

**Not in this milestone, with the reason**: Postgres/Turso and TypeDB/Helix (P-19/P-26: no servers here — the SQL source trait is ready, drivers wait for an environment that can run them), Android build (no SDK; `mobile` builds), JS-free desktop (P-29: a rewrite of three JS packages; the traits exist), CRDT text merge (P-30: the entity log's structural merge lands now; content stays patches), 3D force layout in the renderer (z by layer is enough to see structure; a true 3D Barnes–Hut needs an octree).

## Architecture decisions for this milestone

```mermaid
flowchart LR
  W[Workspace writes] --> LOG[(EntityLog .moonkale/history.jsonl)]
  GIT[Changes panel commit] -->|Checkpoint| LOG
  LOG --> HP[History panel: events, text_at, checkpoints]
  CL[web client] <-->|/api/presence ws| HUB[presence hub per room]
  HUB --> SB[status bar / tab badges / explorer marks]
  GP[Graph panel: 3D toggle] --> R[graph-render: layers, perspective, fog]
  SRV[/api/ext/module] --> WK[Worker: WebAssembly.instantiate + SAB mailbox]
  WK <-->|host calls| WS[Workspace remote sources]
```

- **The log is a file in the workspace** (JSONL, one event per line, appended by rewriting through the folder source): committable, diffable, indexable; snapshots are not needed at this size.
- **Content events carry the patch, not the text**; `text_at` replays from the last `Add` (which carries the initial text hash and length only — the first Content after Add carries a whole-document patch so replay has a base).
- **Presence is a room per folder id**, hub state in memory, no persistence; the message types are the session enum's style (typed, serde).
- **3D is a camera and a z coordinate**, not a second renderer: the same instanced pipelines get a perspective matrix and per-instance depth; the CPU layout stays 2D per layer.
- **The browser runtime reuses ABI v1 unchanged**; the synchronous `call` import is served by blocking the Worker on `Atomics.wait` while the main thread answers — the one place where a Worker is mandatory.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | `core::graph::history` (events, log, fold, text_at, JSONL); `settings.user.name`; workspace appends on every write; checkpoints from the Changes panel; History panel | core, ext-api, ui, extensions/git | unit: fold/replay/round-trip; E2E: edit+save+rename → events, text at an earlier event |
| 2 | presence hub on `api`, client join/leave/update, status bar + tab + explorer marks | api, ext-api, ui, web | E2E: two browser contexts see each other |
| 3 | 3D mode in `graph-render` (layers, perspective camera, fog, projected labels/hit test) + toolbar toggle | graph-render, editors/graph | unit: projection; E2E: toggle → `data-mode=3d`, nodes still hoverable |
| 4 | browser wasm runtime: `js/wasm-host` Worker + mailbox, `/api/ext/module`, COOP/COEP, client `wasm_run` path, permission check | js/wasm-host, ext-host (assets), api, web | E2E: wordcount runs with the server's `run` endpoint blocked |
| 5 | verify, log, vault, ADR-0012 accepted | | |

## Risks
| risk | mitigation |
|---|---|
| Rewriting `history.jsonl` on every save is O(log) | fine below ~10k events; note compaction as the next step; never block the save on the log write |
| Presence hub and the auth gate | the websocket goes through the same middleware (cookie); rooms are folder ids the server already jails |
| WebGL2 depth with alpha-blended edges | sort edges back-to-front per frame in 3D; nodes keep depth test |
| `SharedArrayBuffer` needs COOP/COEP, which blocks cross-origin resources | the app loads nothing cross-origin; headers are set only when the browser runtime is enabled (`MOONKALE_BROWSER_WASM=1` default on) |
| Firefox headless and `Atomics.wait` in Workers | supported; the E2E proves it, and the server path stays as fallback |
