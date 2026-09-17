---
title: "P-045 — Cross-window drag and drop: what failed, what works, what is unverified"
tags: [problem, session, desktop, web]
status: resolved
phase: 1
---
**Status:** resolved — confirmed working on desktop by hand (2026-09-17) after the tab-drag + registry-seeding changes · **Related:** [[Collaboration]], [[Debugging and Logging]]

## Reports (2026-09-17, manual testing by Daniel)

1. **Web (Firefox)**: dragging an editor tab into the other window showed a Firefox error page *"Server Not Found — can't connect to the server at wb-tab-editor-7729155e-…"*.
2. **Desktop**: drag and drop "still doesn't work" (no further output available).
3. **Desktop**: "the opened project doesn't seem to synchronize" — the second window did not show the folder opened in the first.

## Diagnosis

| # | cause | fix |
|---|---|---|
| 1 | `wb-tab-editor-<uuid>` is the workbench tab's DOM id: `dioxus-workbench` makes every tab an HTML5 drag carrying its id as `text/plain` (Firefox needs data or no drag starts). Moonkale's cross-window drag was wired to a *separate* `⋮⋮ path` handle, not the tab — so nothing handled the drop in window B and **Firefox's default for unhandled dropped text is to navigate to it**. | The tab is now *the* cross-window gesture: a native `dragstart` listener maps a `.wb-tab` id to the open document (`Workspace::start_drag_from_tab`, matching the node uuid inside the id — no coupling to the editor's panel scheme). A window-level `dragover`/`drop` `preventDefault` makes stray drops inert. The custom handle was removed. |
| 2 | Cannot be diagnosed without output. Two candidate causes: (a) WebKitGTK may not deliver an HTML5 drop between two webviews of one process (wry installs its own GTK drag handler); (b) the in-process bus message may not have woken the other window's VirtualDom. | Diagnostics added: every session message is logged (`session[<window>] send/recv …`) to the `dx serve` terminal, and the status bar shows **"N windows"** once peers have answered. The **"Move it here" banner** after `dragend` makes the move work even if (a) holds. |
| 3 | On desktop the new window learned sources only through `Hello → SourceOpened` replies from the other window, i.e. it depended on the other window's VirtualDom being polled for a message from a different runtime — the same suspicion as 2(b). | The desktop `session()` now **seeds the new window from the process-wide `SourceRegistry`** before it even says `Hello`, so folder sync no longer depends on the bus at all. |

## Verified (web, Playwright with real Firefox)
`packages/web/tests/e2e/session.mjs`: New Window → both status bars show "2 windows" → **real mouse drag of the editor tab** in one window → drop target in the other → drop moves the document → drag released without drop → banner → *Move it here* moves it back → a simulated stray drop of `wb-tab-editor-xyz` does **not** navigate → dismiss.

## Desktop checklist (kept for regressions)
Confirmed working by hand. If it regresses, this is what to look at when running `cd packages/desktop && dx serve --platform desktop`:

1. Open a folder, *View → New Window*. Does the second window show the folder tree? (Registry seeding — should now be yes regardless of the bus.)
2. Does the status bar of **both** windows say **"2 windows"**? If either says "1 window", the in-process bus is not delivering; paste the `session[…]` lines from the terminal.
3. Drag the editor **tab** from window A over window B. Either B shows the blue drop overlay while dragging (HTML5 drag crosses windows on WebKitGTK ✅) or nothing changes until you release, and then B shows the *Move it here* banner (bus works, OS drag doesn't cross ➜ fallback path). If neither happens, paste the terminal lines.

## Deferred
Two-monitor use (a window per screen) is exactly this feature; nothing more is needed once the desktop path is confirmed. Kept in [[Problem Ranking]] under P-34 (presence) as the multi-window baseline.
