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

## Quantized-clock fallback — 2026-10-15

A locked, quiet-machine release desktop rerun of all 21 cases confirms that **both** public clocks produce integer-millisecond intervals. Switching to `performance.now()` alone does not resolve the gate. This agrees with [WebKit's Performance implementation](https://github.com/WebKit/WebKit/blob/main/Source/WebCore/page/Performance.cpp), which reduces clock precision. No WebView security/isolation settings are changed for the benchmark.

Use the refresh-relative alternative authorized in NEXT-2: collect up to 120 idle callback intervals before the first intended gesture, require at least 30, and use their median as the refresh baseline. A frame is missed above 1.5 times that baseline; p95 must stay at or below that threshold and max must remain at most 50 ms. A baseline slower than 17 ms is rejected, preventing a busy/slow display from relaxing the target. The driver waits 1.2 seconds after arming to collect the idle baseline. Reports retain raw callback and rAF intervals, the idle samples, detected quantization, refresh interval, threshold, missed count and gate name. Quantization is detected conservatively when all callback deltas are integer within floating-point tolerance.

For fractional-clock recordings, `passed` continues to use the original 16.7/50 ms gate. Quantized recordings use `gate: "calibrated-vsync"`; their original interval gate is separately reported as `strictPassed: false`. A calibrated pass is **not evidence of sub-millisecond p95 ≤16.7 ms**. WP-02's original strict desktop requirement and Daniel's hardware desktop check remain open; do not silently treat the fallback as satisfying them.

Raw pre-optimization rerun: `/tmp/moonkale-desktop-fps-corrected-20261015{.json,.summary.json}`. All 21 gestures interacted; none passes the old integer-clock gate. Source typing p95/max 22/25 ms; documentation-preview typing 33/41 ms. Preview wheel max 178 ms and 3 MB wheel max 149 ms are real stalls, not explained by 1 ms quantization. Software GL still covers CPU-side work rather than hardware GPU compositing.


### Bounded private-cache follow-up — 2026-10-15

With master `0aef7472` (private persistent cache/data/config homes), the release fixture was rebuilt. One three-case wheel warm-up attempt and one full 21-case guarded attempt both aborted before launching a case: aggregate CPU never met two consecutive <=10% samples in six attempts. No cache-warm comparison or new frame result is available; neither attempt proves or disproves removal of the prior210ms preview-wheel pause. Raw preflight samples and logs: `/tmp/moonkale-next3-{warmup,desktop}*`. No further headless gate iteration is planned in this run. The real GPU/desktop check remains Daniel's final acceptance; its commands, files, seven gestures and strict versus calibrated criteria are in the coordinator handoff. WP-03 proceeds through its own milestone gates while this desktop acceptance remains explicitly open.


## Idle-callback calibration correction — 2026-10-22 (P-225)

Daniel's 2026-10-21 KDE/NVIDIA/60 Hz reports contain 1–16 ms idle callback
spacings (one sample's median is 8 ms), followed by steady 16–18 ms gesture
frames. A median of those idle callbacks is not a refresh estimate. The FPS
host contains one recorder rAF loop; clicking Record and waiting four seconds
on private sway/Mesa did not reproduce the short callbacks. The chronological intervals alternate in pairs (13+3, 14+2, 15+1 ms),
consistent with two out-of-phase scheduling sources splitting one cadence.
WebKitGTK 2.54.1's [render scheduler](https://github.com/WebKit/WebKit/blob/webkitgtk-2.54.1/Source/WebCore/page/RenderingUpdateScheduler.cpp)
has both a display monitor and a timer fallback; the pairing is evidence for a
scheduling hypothesis, not proof that either path is at fault. WebKit's
[GTK/WPE timing issue](https://bugs.webkit.org/show_bug.cgi?id=238999) also
explains why rAF timestamps cannot be equated with physical refresh.
The precise hardware/backend scheduling cause remains unproven; there is no evidence of a
second recorder loop. Reports now also retain idle rAF deltas to distinguish
callback scheduling from the rAF clock on the next hardware run.

Use the idle **95th percentile** as the upper-envelope cadence estimate, replacing
the idle median above. Short inter-refresh callbacks no longer dominate it;
long idle stalls remain conservative calibration failures. Retain at least 30
samples, reject baselines above 17 ms, and leave both gates unchanged: fractional
p95 ≤16.7 ms/max ≤50 ms; quantized p95 ≤1.5×baseline/max ≤50 ms. The raw polluted
sample regression estimates 15 ms and accepts ordinary 60 Hz frames while
rejecting 32 ms frames. This is a scheduling estimate, not physical presentation
timing or proof of the original strict desktop requirement.

Interaction evidence counts actual arrow traversals and requires a changed
caret or viewport, because Daniel's WebKitGTK reports delivered 91–112 arrow
keydowns with no repeat flags. Small-scroll recording counts ≤10 pixel document
viewport movements, including smooth scrolling from larger wheel deltas, and
requires trusted wheel input. Both axes use that same viewport; textarea-sink
scroll events and wheel input with no document movement are rejected. Raw wheel
axes/units and accumulated viewport travel are retained. The old 3 MB vertical
wheel attempts with zero scroll events remain invalid interactions.
