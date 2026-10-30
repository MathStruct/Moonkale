---
title: "P-242 — Sharing announcement test outruns remote position"
tags: [problem, testing, sharing]
status: resolved
phase: WP-26
---
**Status:** resolved · **Related:** [[Shared Work]], [[Agents in the UI and on the Graph]]

## Problem

CI's sharing suite timed out in an unlabelled `waitForFunction`, with Alpha.md
selected. A local 6× CDP CPU-throttled run reproduces this at the second requested
cursor announcement. The test moved Ada's caret, waited 120 ms, and requested
Bob's current snapshot before Bob necessarily received the new position. Waiting
for the announcement to differ could then time out although the requested
snapshot was correct.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Increase the sleep | Small change | Machine-dependent ordering remains |
| B | Observe the exact remote member, source, document and changed position | Verifies the transport boundary | Requires received-frame diagnostics |

## Measurements

Chromium, debug wasm, software GPU, CDP `Emulation.setCPUThrottlingRate` rate 6
on both independent participant pages. The labelled failure is
`requested announcement changed`; both participants completed caret, pointer,
zoom, follow and cancellation checks first. Ten 6× runs passed on 2026-10-30 (both editor implementations per run).

## Decision

Choose B. Label every explicit wait, apply optional throttling to every page,
and observe received presence payloads before requesting an announcement of a
new cursor. Keep the assertion that cursor movement alone never announces.
Pointer projection waits target Ada's actual marker; follow waits target the
requested selected tab. No timeout increase or dependency change is needed.

Review of the production paths: `RemotePresence` replaces its one pending sample
with the latest state, including revocation; heartbeat samples are taken from
current workspace state. CodeMirror's remote-mark effect subscribes to editor
readiness and presence, so it retries a pre-mount position once ready. Neither
path explains this reproduced timeout. This evidence does not establish that
all possible presence ordering problems have been eliminated.

## Lesson

A requested snapshot can only reflect remote state already received. Synchronize
the test on the particular participant and position, and label waits before
investigating an otherwise opaque CI timeout.

Validation: runs 1–10 each reported `SHARING E2E: PASS`; raw suite summaries are
`target/next44-sharing-6x-{1..10}.log`. The runner was frozen after a source edit
interrupted its shell after run 6 had already passed. Runs 7–10 used that frozen
runner, all returning zero. The relevant summaries are
`target/next44-sharing-ten.log` and `target/next44-sharing-four.log`.
