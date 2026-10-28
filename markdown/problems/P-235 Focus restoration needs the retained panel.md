---
title: "P-235 — Focus restoration needs the retained panel"
tags: [problem, ui, focus]
status: resolved
phase: WP-64
---
**Status:** resolved · **Related:** [[ADR-0027 Simple by default]], [[Work Packages]]

## Problem

Open Alpha from the palette, run Show Graph, then close Graph. The workbench
can select its first retained tab (Settings), leaving Alpha mounted but hidden.
Calling `focus()` on Alpha's saved input leaves focus on the document body.
The next typed character consequently starts global search instead of editing.
A saved DOM node alone is not enough to restore a command's opener.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Focus the saved element | Small; works within the same visible panel | Cannot reveal a retained hidden panel |
| B | Restore an entire saved layout | Reveals the old arrangement | Overwrites later layout choices |
| C | Activate the saved panel through its existing tab, then focus its control | Preserves current layout and ordinary workbench navigation | Must wait for rendering and avoid racing new command focus |

## Measurements

The WP-64 Firefox discovery journey at 1500 × 900 reaches this path with
Settings, README, Alpha and Graph retained in one tile. Before restoration
activated the opener's panel, the 30-second focus assertion after Graph closure
failed with the body focused and Settings selected. Both interaction journeys
passed independently. The Chromium graph accessibility suite additionally
checks search and return focus on the real canvas.

## Decision

Use option C. The shell retains the opener's element, stable id and containing
panel. Closure waits for pending command focus before restoring a stranded
focus, activates the saved panel through the workbench tab if hidden, then
focuses its original control with bounded render retries. This prevents palette
closure from overriding focus intentionally requested by a new panel.
Document navigation focuses its editor; explicit tab navigation retains tab
focus. Selecting an already-active document does not start a redundant async
open. Inline editor bars remain nonmodal; modal dialogs contain Tab.

Validation: the discovery/focus regression, both interaction journeys, all
48 browser configurations and all 18 gate commands pass. The NEXT-29 handoff
records their feature sets, logs and the hold-Ctrl screenshot.

## Lesson

Returning focus includes restoring the visible navigation context. Keep layout
ownership in the workbench, preserve a new command's own focus request, and test
closure when the previous panel is retained but hidden.
