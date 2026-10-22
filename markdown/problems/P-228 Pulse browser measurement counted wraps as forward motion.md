---
title: "P-228 — Pulse browser measurement counted wraps as forward motion"
tags: [problem, graph, testing]
status: resolved
phase: WP-04
---
**Status:** resolved · **Related:** [[Work Packages]], [[How to Write Tests]]

## Problem
`graph-pulse.mjs` took the largest absolute peak displacement across six pairs
of screenshots. A pulse wrapping from the end to the start of an edge could be
counted as a large forward displacement. Slow and fast pulses were also sampled
in separate captures/time windows, so their maxima did not measure comparable
speeds. The existing shader uses periodic forward motion; no pulse renderer
behavior changed in WP-04.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Sample both edges in shared frames, reject wraps and compare velocities | Measures the intended behavior and retains speed/LOD assertions | More profile samples |
| B | Retry until a favorable phase passes | None | Would hide the measurement error; rejected |

## Measurements
Observed in NEXT-11 on 2026-10-23, including a run holding the bench lock and both
heavy slots: the old test reported slow 63 px versus fast 36 px. An earlier run
passed with the same renderer. Absolute maxima across phase wraps are not speed.

## Decision
The fixture now pins equal-length edges. Thirteen shared captures sample both
brightness profiles at finer resolution. Boundary/off-stroke peaks and backward
wrap intervals are excluded; median forward screen velocities are checked
against each declared speed and the 3:1 ratio. The zoomed-out animation LOD check
remains. No renderer pulse behavior or numerical speed is changed.

## Lesson
Periodic motion needs directed, wrap-aware measurements over shared intervals.
A maximum absolute displacement is neither an unwrapped distance nor a velocity.
