---
title: "P-225 — Native FPS recorder misclassifies idle cadence and input"
tags: [problem, editors, testing]
status: resolved
phase: WP-02
---
**Status:** resolved (recorder accounting; hardware rerun due) · **Related:** [[ADR-0023 Native editor frame measurements]]

## Problem
Daniel's 2026-10-21 KDE/NVIDIA/60 Hz recordings held 17–18 ms p95 and at most
48 ms, but idle medians of 8–11 ms classified ordinary frames as dropped.
Idle callback intervals alternate in pairs summing to about 16 ms. Held arrows
produced 91–112 keydowns with no `repeat` flags. Smooth scrolling produced many
viewport scroll events from only 3–4 larger wheel events; none met `smallWheel`.
Some 3 MB wheel attempts delivered wheel events but no viewport movement.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Upper-envelope idle calibration; count traversal and actual viewport movement | Handles observed native delivery; keeps timing thresholds and rejects no-op gestures | Scheduling estimate; physical presentation timing remains unmeasured |
| B | Relax frame or interaction gates | None | Would hide missed frames or missing gestures |

## Measurements
Raw reports remain read-only in `~/.cache/moonkale-fps-real/reports/{rust,threemb}.json`.
The polluted rust.json[6] sample's idle p95 is 15 ms rather than its 8 ms median.
The regression accepts 60 Hz gesture frames and rejects 32 ms frames. Clicking
Record and waiting four seconds on private sway/Mesa did not reproduce the
paired callbacks; precise KDE/NVIDIA scheduling cause remains unproven.

Nineteen clock/input regressions pass. Browser transport plus trusted arrow and
small-scroll checks pass using the current recorder against the existing panel
bundle. Fresh native source/preview/3 MiB runs validate six recorded interactions;
each arrow case observes 112 keys and each small-scroll case 63 viewport moves.
These observer-enabled development recordings are correctness evidence only.

## Decision
Use idle p95, retain the ≥30 sample / ≤17 ms calibration guard and both existing
frame gates. Retain idle rAF deltas for follow-up. Count actual arrow inputs plus
caret/viewport change, independent of `repeat`. Count actual small viewport
movements on either axis with trusted wheel evidence. Ignore sink-only scrolls
and retain raw wheel axes/units and accumulated viewport travel. No-op 3 MB
wheel attempts still fail. See [[ADR-0023 Native editor frame measurements]].

## Lesson
Event flags and wheel deltas are backend-dependent. Observe the document's
geometry as well as trusted events; callback cadence is not physical refresh.
