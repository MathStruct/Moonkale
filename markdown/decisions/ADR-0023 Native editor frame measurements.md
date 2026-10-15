---
title: "ADR-0023 Native editor frame measurements"
tags: [adr, editors, performance, testing]
---

## Status

Accepted measurement design, 2026-10-14. Desktop release acceptance remains open.

## Context

[[Code Editor Requirements]] CED-R22 requires p95 frame intervals at most 16.7 ms and no frame over 50 ms during typing, held arrows, scrolling, selection dragging and caret movement. It applies to highlighted source and spec [[018]]'s 3 MB document, on release web and desktop builds. Input latency alone does not expose renderer stalls.

## Decision

`editor-fps.mjs` is a standalone Playwright suite. It records RAF callback intervals, p50/p95/max, supported long-animation-frame/longtask observations, actual build labels and browser version. It measures all seven gesture variants on the same 5000-line Rust file with documentation previews off/on, and on the 3 MB text file. Scroll and selection checks reject gestures that do not interact with the editor. Development recordings are diagnostic evidence; strict acceptance requires a release build. The 16.7/50 ms limits remain unchanged.

Desktop uses the production native panel in the release WebView fixture. `MOONKALE_FPS_SOURCE` supplies the same corpus file and `MOONKALE_FPS_REPORT` enables a manual recorder. It observes trusted input and RAF intervals for five seconds from the first intended gesture, records WebView/viewport metadata, and writes bounded JSON samples. It injects no input. Missing/insufficient gestures and development builds cannot pass. The private-display harness may supply compositor input through `tools/headless.sh`; the recorder itself only observes events. Daniel tests real input on his own desktop.

Run performance acceptance separately from builds and other browser work. Retain the raw report outside the repository and record its aggregate results in [[Rust Code Editor Roadmap]]. Keep correctness, byte-exact round trips and the existing same-file relative latency checks alongside the frame gate. The standalone suite stays outside `run-all.sh`'s strict list until both platform gates have evidence.

## Consequences and evidence

The initial release web run exposed 233 ms p95 typing in the 3 MB file, up to 83 ms p95 preview scrolling and a 117 ms held-arrow stall. Profiling found an unbounded cursor-word status display, fractional scroll snapping, repeated height notifications and source-cell DOM churn. A full-sequence Chrome timeline identified 62 ms garbage collection after scrolling.

Bounded engine-owned word slices, retained fractional offsets, coalesced height updates, keyed virtual rows and same-presentation ASCII text runs address those costs. Text runs split at Unicode/tabs, wiki, caret, search and other presentation boundaries; Rust still owns exact scalar hit testing and selection. The release web rerun passed all 21 cases, each at about 16.67 ms p95, with a longest frame of 33.33 ms. This is web evidence only; desktop acceptance remains unmeasured.


## Measurement amendment — 2026-10-15

The v4 headless desktop report contains integer-millisecond rAF deltas: 15 of 21 cases fail, including otherwise steady cases with p95 = 17 ms. A 60 Hz interval is 16.666… ms, so the rounded rAF argument cannot resolve the 16.7 ms gate. Desktop now measures unrounded `performance.now()` deltas at callback entry and retains the original rAF deltas as `rafIntervals`, with `clock: "performance.now"` in the report. Web keeps its existing sub-millisecond rAF timestamps. Callback-entry spacing includes callback scheduling delay; it must not be confused with physical presentation timestamps. The p95 ≤ 16.7 ms and max ≤ 50 ms limits are unchanged; no refresh-relative relaxation or rounding of measured intervals is applied. If the WebView also quantizes `performance.now()`, those measurements remain inconclusive and need another clock or a separately agreed refresh-based gate.

The old report still identifies likely real CPU-side misses: typing p95 20 ms (Source) and 23 ms (documentation preview), with maxima 24/27 ms; preview wheel max 23 ms and scrollbar-drag max 21 ms. Do not reclassify these old samples as passing under the new clock. Rerun and profile before changing the renderer.

Headless sway uses pixman and Mesa software GL because the NVIDIA WebKit path crashes in `libnvidia-eglcore`. These runs measure CPU-side Rust, DOM and layout costs, not hardware GPU compositing. Passing them is necessary, not sufficient; Daniel’s real-desktop release check remains the final acceptance. The amended recorder has not yet been validated in the desktop WebView: the private runtime fix from master now permits display startup, and the repository gates and release desktop rebuild pass; Git staging still reports a read-only index, and the required `~/.cache/moonkale-bench.lock` cannot be created in this sandbox. The recorder transport check passes, but timing acceptance and profiling remain pending. WP-02 and WP-03 remain open.
