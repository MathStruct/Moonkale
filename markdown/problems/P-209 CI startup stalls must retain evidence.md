---
title: "P-209 — CI startup stalls must retain evidence"
tags: [problem, testing, ci]
status: resolved
phase: WP-26
---
**Status:** resolved (startup recovery and evidence) · **Related:** [[Work Packages]], [[Testing Strategy]]

## Problem
Commit 5a4c012b's structural browser CI job stalled for 1200 seconds without log
or target changes. Its final output was successful wasm-bindgen/esbuild installation,
not a compiler failure. The default browser job started from a restored cache.
The original cause is unproven. The screenshot-only artifact contained no files,
so the full startup log and owned process states were not archived.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Increase inactivity allowance | small | hides real hangs and loses evidence |
| B | One bounded stall retry plus diagnostics | can recover transient startup, preserves evidence | a repeated stall still fails |
| C | Upgrade Dioxus speculatively | may change behavior | no established cause, broad change |

## Measurements
Failed run: https://github.com/MathStruct/Moonkale/actions/runs/38903191651
(job 116980223694). All other CI jobs passed. Local shell tests cover real isolated
process trees with fake time/HTTP: success, compiler failure, process exit,
one recovered stall, repeated stalls, a deadline shared between attempts,
default single attempt, and refusal/preservation of missing or mismatched process identity.

## Decision
Choose B. CI explicitly requests noninteractive, verbose, locked startup. Default
local startup has one attempt; CI has two. Only inactivity retries, inside the
existing 3600-second total deadline; the 1200-second inactivity threshold is
unchanged. Each failed attempt records process/disk/progress diagnostics and
stops only its owned process group. Logs for both attempts survive a retry and
are uploaded with screenshots even after recovery. No environment/argument dump.
The original upstream stall is not claimed diagnosed by this infrastructure fix.

## Lesson
A banner or tool-install message does not prove compilation began. Bound recovery,
retain the first failure, and keep compiler errors distinguishable from inactivity.
