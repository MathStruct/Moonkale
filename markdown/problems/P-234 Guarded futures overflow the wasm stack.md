---
title: "P-234 — Guarded futures overflow the wasm stack"
tags: [problem, reliability, testing]
status: resolved
phase: WP-57
---
**Status:** resolved · **Related:** [[Crash Safety]], [[How to Write Tests]]

## Problem

After WP-57, Chromium's renderer died at the first structural Preview click in
both `structural-edits` and `graph-rename`, with native and CodeMirror editors.
CI used `SERVE_FEATURES=julia,structural-edits`; the smoke feature set did not
exercise these paths. Playwright reported `Target crashed`, not an assertion.

## Options

| | Approach | Benefit | Cost |
|---|---|---|---|
| A | Heap-pin input futures before building guard wrappers | Bounds wrapper size for every task and resource | One allocation per guarded input |
| B | Box only structural preview | Fixes this caller | Other large extension futures retain the same failure |
| C | Increase the wasm stack | More headroom | Leaves inline amplification and platform-dependent failures |

## Measurements

Reproduced 2026-10-28 with CI's exact structural features. Both structural
suites crash with both editors; `group-history` and `graph-a11y` pass. A diagnostic
Chromium run reports roughly 10.7 MB used JS heap immediately before Preview,
no JS exception or panic-hook location, and a renderer crash before the preview
planner request. The input preview future is 19,008 bytes on wasm (20,800 native).
Generated wasm prologues include a **171,168-byte** frame in the original preview
spawn wrapper. Nested async guards carry repeated inline copies of their input
through construction and polling, exhausting the wasm stack.

Heap-pinning before `spawn` alone makes the structural suite pass and reduces
its spawn entry frame to 19,408 bytes. Restoring the original wrappers restores
the crash. The final implementation applies the same boundary to all helpers: its preview
spawn entry frame is 19,504 bytes and subsequent Dioxus spawn wrapper frames
are 272–288 bytes, versus about 76 KiB each before the fix.
Diagnostic logging and the temporary size probe are removed from production.

## Decision

Option A. A shared `guarded_future` synchronously pins the input on the heap,
then returns a small poll adapter. `ExtensionGuard::future` and `task`, scoped
and session spawns, resource/background-future factories and async event
callbacks construct their wrappers around that adapter. Every poll still sets
and restores the owner, catches native unwinds and stops disabled siblings.
Factory creation remains guarded and resource dependencies remain reactive.
The planner, panic hook and crash overlay require no changes.

The regression supplies a 64 KiB future, checks that guard wrappers stay below
1 KiB, checks lazy construction and Pending → Ready without duplicate polling,
and verifies owner restoration, disabled-sibling cancellation and exact drops.
Its size assertion fails on the original implementation. `crash-safety.mjs`
also accepts `CRASH_BROWSER=chromium` to verify actual event/task wasm panics
reach the independent overlay in the browser that exposed this regression.

## Lesson

An async guard can multiply its input's stack footprint even when its logic is
correct. Pin large inputs **before constructing** wrapper state machines;
pinning inside another async body is too late. Include the experimental feature
sets and both editors when validating shared execution infrastructure.

## Verification (2026-10-28)

All AGENTS.md gates pass, plus structural server/graph tests and clippy,
desktop structural and web structural checks, six native crash tests and nine
server-startup harness tests. The original guard fails the new size regression;
restoring it also restores the Chromium preview crash.

Browser feature sets were built and served separately:

- `julia,structural-edits`: structural edits and graph rename with both native
  and CodeMirror editors, group history and graph accessibility pass.
- `julia,rocksdb,helix,crash-test`: all 12 CI smoke suites pass. Real wasm event
  and task panics reach usable independent overlays in Firefox and Chromium.
- `sophia`: the CI folder-store suite passes.
- Empty `SERVE_FEATURES` (platform defaults): all 18 graph suites pass after
  correcting four pre-existing fixture-readiness races. With both original and
  fixed guards, edit/layers/pulse/flow injected scenes before the initial folder
  graph arrived. They now wait for positive matching host/engine node counts,
  as neighboring graph suites already do. Their assertions are unchanged.

Logs and generated wasm frame measurements are under `target/next28-*` in the
codex worktree; the handoff lists individual files. Test servers were stopped
through `serve.sh stop`. No commits, pushes or GitHub comments were made.
