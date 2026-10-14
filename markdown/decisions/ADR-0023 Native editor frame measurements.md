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

Desktop uses the production native panel in the release WebView fixture. `MOONKALE_FPS_SOURCE` supplies the same corpus file and `MOONKALE_FPS_REPORT` enables a manual recorder. It observes trusted input and RAF intervals for five seconds from the first intended gesture, records WebView/viewport metadata, and writes bounded JSON samples. It injects no input. Missing/insufficient gestures and development builds cannot pass. This follows [[Running Several Agents]] and AGENTS.md's rule that Daniel tests real Wayland input.

Run performance acceptance separately from builds and other browser work. Retain the raw report outside the repository and record its aggregate results in [[Rust Code Editor Roadmap]]. Keep correctness, byte-exact round trips and the existing same-file relative latency checks alongside the frame gate. The standalone suite stays outside `run-all.sh`'s strict list until both platform gates have evidence.

## Consequences and evidence

The initial release web run exposed 233 ms p95 typing in the 3 MB file, up to 83 ms p95 preview scrolling and a 117 ms held-arrow stall. Profiling found an unbounded cursor-word status display, fractional scroll snapping, repeated height notifications and source-cell DOM churn. A full-sequence Chrome timeline identified 62 ms garbage collection after scrolling.

Bounded engine-owned word slices, retained fractional offsets, coalesced height updates, keyed virtual rows and same-presentation ASCII text runs address those costs. Text runs split at Unicode/tabs, wiki, caret, search and other presentation boundaries; Rust still owns exact scalar hit testing and selection. The release web rerun passed all 21 cases, each at about 16.67 ms p95, with a longest frame of 33.33 ms. This is web evidence only; desktop acceptance remains unmeasured.
