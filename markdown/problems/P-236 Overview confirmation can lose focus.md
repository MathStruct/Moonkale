---
title: "P-236 — Overview confirmation can lose focus"
tags: [problem, testing, overview]
status: resolved
phase: WP-26/60
---
**Status:** resolved · **Related:** [[Overview]], [[ADR-0027 Simple by default]]

## Problem

In repeated CI smoke suites, Delete on a dirty overview card opened its close
confirmation while focus could remain in the palette input. Escape then closed
the entire overview instead of cancelling only the confirmation.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Wait for actual dialog focus in the browser regression | Distinguishes a premature key from a broken focus transfer | Exposes the product failure; cannot repair it |
| B | Explicit mounted confirmation focus and input-side Escape cancellation | Dialog owns keyboard input; cancellation retains overview even during transfer | Needs real browser coverage |

## Measurements

2026-10-29, debug smoke features `julia,rocksdb,helix,crash-test`, Chromium,
1800×1100, software GPU: the second initial sweep failed overview.mjs with
0 cards rather than 1 after Escape. After adding an actual-focus wait, the
third fresh sweep timed out for 30 seconds: the confirmation remained visible
without owning focus. Screenshots/logs are in the NEXT-36 handoff.

## Decision

Use B and retain A as the regression: explicitly focus the confirmation button
after mounting; input-side Escape cancels a pending confirmation before closing
the palette. Both cancellation paths return focus to search. No sleeps or weaker
assertions. Five dedicated overview repetitions and three complete 22-suite
smoke sweeps pass with the product fix; all AGENTS gates and the full browser
feature matrix pass after the terminal fixture correction.

## Lesson

DOM attachment and autofocus attributes do not prove that a nested modal owns
focus. Test the active element, and keep cancellation scoped to the pending
confirmation while focus is moving.
