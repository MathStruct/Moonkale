---
title: "Benchmarks — what to measure, how, and against which budget"
description: "The benchmark catalogue for Moonkale: CPU, GPU and frame time, memory, network, storage I/O and load on a shared server. Each has a fixture, a metric, a budget (or \"baseline first\") and where it runs. Also the method (quiet machine, medians, results kept in a database) and what exists today. 2026-10-14."
tags: [testing, performance, benchmarks]
---
**Why (Daniel, 2026-10-14):** a document of benchmarking tasks for Moonkale, covering performance on CPU and GPU, memory consumption, internet usage and load balancing. What lives in memory and what goes to the database is decided beside it, in [[ADR-0026 Memory and the store]].

**Today:** benchmarks exist, but scattered:
- `bench.mjs` (layout step time, 1 k to 100 k nodes);
- `bench-draw.mjs` (draw time per frame, SwiftShader);
- spec [[018]]'s 3 MB typing latency (87 ms per key);
- the code editor's 60 FPS gate (CED-R22, [[Rust Code Editor Roadmap]]);
- ratio bounds inside `live-preview-long.mjs`;
- the 50 000-row scroll in `table-grid.mjs`.

None of them keeps its results over time. This page turns them into one catalogue, `BM-nn`.

## Method

- **The reference machine** is the development box: 8 cores, 30 GB RAM, an RTX 3080 (10 GB), NVMe, Linux on Wayland. Plus the phone (Galaxy S10e) for mobile budgets, and CI runners **only for coarse regressions** (they're noisy and have no GPU).
- **A quiet machine.** A benchmark takes the bench lock (`flock ~/.cache/moonkale-slots/bench.lock`, beside the build slots so a sandboxed agent that may write that one directory can take it) and refuses to run while builds or other agents' browser suites are running (see [[Running Several Agents]]). Numbers from a busy machine are not recorded.
- **Medians over runs:** a warm-up first, then N runs (N ≥ 5), recording median, p95 and max. Frame metrics use `requestAnimationFrame` intervals, latency uses `performance.now()` around the action, and memory uses RSS (native) and `performance.measureUserAgentSpecificMemory` (web, where available).
- **Results are kept in a database, not in logs:** each run appends a row (benchmark, commit, platform, metric, value, timestamp) to `~/.cache/moonkale-bench/results.sqlite`. Moonkale opens that file as a source, so the history is a table and, with the plot extension, a chart ([[Plotting Requirements]]). A release's summary row goes into this page.
- **Budgets:** a benchmark with a budget fails when over it. Where none is set, the first job is a **baseline**; the budget follows from it and from use.

## The catalogue

### CPU and latency

| BM | What | Fixture | Metric | Budget | Exists |
|---|---|---|---|---|---|
| 01 | Startup, desktop | the release app, the Default session with this repository | to first paint; to interactive (a keystroke lands) | baseline first | — |
| 02 | Startup, web | the web build, cold cache and warm | the same, plus bytes loaded (BM-30) | baseline first | — |
| 03 | Open a folder and index it | this repository; a synthetic 100 k-file tree; a drive root (P-195) | time to the first tree listing; time to the index's end; files per second | first listing < 200 ms | — |
| 04 | Typing in a large document | spec 018's 3 MB file, CodeMirror and the Rust editor | key to dirty state; key to frame | 87 ms per key recorded; CED-R22 below | ✅ (spec 018) |
| 05 | Search | the index of this repository | query to first results (BM25; hybrid with embeddings) | < 100 ms BM25 | — |
| 06 | Table operations | a 50 000-row result | sort, filter, scroll to the end | sort < 100 ms; scroll frames below | partly (`table-grid.mjs` times scroll) |
| 07 | Flow validate and generate | a 1 000-block flow, a nested subsystem, the RxInfer coin | `validate` time; codegen time | < 50 ms for 1 000 blocks | — |
| 08 | Markdown render | a 229 KB note (live preview), a note with 500 formulas | parse; render; math (KaTeX and Typst, WP-36) | — | partly (`live-preview-long.mjs`) |
| 09 | Graph layout | 1 k, 10 k, 50 k, 100 k nodes | ms per layout step | — | ✅ `bench.mjs` |

### GPU and frame time

| BM | What | Fixture | Metric | Budget | Exists |
|---|---|---|---|---|---|
| 10 | Code editor frames (CED-R22) | an ordinary file and the 3 MB one: typing, held arrows, scrolling, selection drag | p95 and max frame interval | **p95 ≤ 16.7 ms, max ≤ 50 ms** | planned (WP-01) |
| 11 | Graph draw | 100 k and 1 M nodes (coarse tier), idle, panning, hovering | draw ms per frame; frames per second | 60 FPS at 100 k on the RTX 3080 | ✅ `bench-draw.mjs` (SwiftShader only) |
| 12 | Graph on the phone | 10 k nodes on the S10e | FPS while panning and pinching | 30 FPS | — |
| 13 | Flow canvas | 1 000 symbol blocks with captions, panning and dragging | frame interval (the overlay's 2D strokes are on the CPU) | p95 ≤ 16.7 ms | — |
| 14 | Table scroll | 50 000 rows × 60 columns | frame interval while scrolling | p95 ≤ 16.7 ms | — |
| 15 | Live run drawing | a Lenticulum or RxInfer run streaming 10 k belief updates per second (LT-10) | frames per second; update lag | 60 FPS, lag < 100 ms | — |

### Memory

| BM | What | Fixture | Metric | Budget |
|---|---|---|---|---|
| 20 | Idle | the desktop app with the Default session; the server with no client | RSS | baseline first |
| 21 | Per folder | the index per 10 k files | bytes per file in the index | — |
| 22 | Per graph node | the engine at 100 k and 1 M nodes | bytes per node, CPU and GPU side | — |
| 23 | Per table row | 50 000 rows | bytes per row (`Value`) | — |
| 24 | The web client | the wasm heap after opening this repository and the graph | bytes | — |
| 25 | **No growth** | open and close a folder, a graph, a table, a conversation 50 times | RSS after the 50th minus after the 1st | **≈ 0 (no leak)** |
| 26 | The phone | the release APK with a vault | RSS; whether Android kills it in the background | under the S10e's per-app limit |

### Network

| BM | What | Fixture | Metric | Budget |
|---|---|---|---|---|
| 30 | First load | the web client, cold | bytes transferred, compressed (wasm, JS bundles, fonts, KaTeX) | baseline first, then a budget per bundle |
| 31 | Per keystroke | typing in a remote document (spec 018's splices) | bytes per key up and down | splices only, never the document |
| 32 | Idle | an open session doing nothing for 10 minutes | bytes per minute (presence, `changes_since` long polls, keep-alives) | — |
| 33 | Graph and table sync | opening the graph of this repository; a 50 000-row result | bytes per sync | — |
| 34 | Agent turns | one conversation turn with tools | tokens in and out, bytes, cost; shown to the user per conversation | — |
| 35 | External data | a day of futures from Databento ([[Trading with Databento]]); a cached repeat | bytes, cost; the repeat costs nothing | cached repeat = 0 |
| 36 | Bad networks | 3G-like latency and 1 % loss (throttled in Playwright), and offline | does the UI stay responsive; does it recover | no frozen frame |

### Storage I/O

| BM | What | Metric |
|---|---|---|
| 40 | State store writes: settings, layout, an unsaved-buffer snapshot, an entity-log event | latency median and p95; never on the input path |
| 41 | Session switch ([[ADR-0025 Sessions are the outermost state]]) | save and restore time for a session with 20 documents |
| 42 | Caches | size growth per day of use; eviction keeps them under budget |

### Load on a shared server

| BM | What | Fixture | Metric |
|---|---|---|---|
| 50 | Many clients | 1, 10, 50 simulated web clients on one server: browse, edit, query | latency p95 per action; CPU and memory per client |
| 51 | Many agents | 1, 5, 20 conversations running turns with tools | turn latency; how fairly they share CPU and the provider's rate limit |
| 52 | Tools beside people | an index rebuild and a compile during editing | the editing client's latency stays within its budget (SW-45: humans first) |
| 53 | Backpressure | more requests than the server can serve | queues are bounded; excess is refused with a clear error, never a hang |
| 54 | Reader mode | anonymous visitors querying a published source ([[Publishing Sources]]) | caps hold (rows, bytes, time); one visitor can't starve others |

## Order (proposal)

1. **The method first:** the bench lock, the results database, and a runner that records.
2. **Move the existing benchmarks into it:** BM-04, 09, 11, and the scroll part of BM-06.
3. **The frame gates:** BM-10 (codex's WP-01), BM-11 on the real GPU, BM-13 and BM-14.
4. **No growth:** BM-25 catches leaks early and cheaply.
5. **Network on the web build:** BM-30 to 32.
6. **Server load:** BM-50 to 54, before a shared server is offered to anyone.

Work package: WP-55 in [[Work Packages]].

## WP-69 development profiles — 2026-10-17

Development builds optimize the Tree-sitter runtime and dioxus-code syntax-query dependency at level 2, and the code-view crate at level 1. The standalone native fixture mirrors these overrides because it has its own workspace. This is a developer-experience change: parsing and preview indexing remain responsive while exercising ordinary development builds. It trades some compilation time and precise debug stepping for faster input-path work. Release and graph-wasm profiles are unchanged.

These overrides are a separate proposal from the code fix in [[P-211 Preview row queries and folding repeated full text work]]. Validate `live-preview-long` with the code changes and the original development profile first; record the same code with these overrides separately. A compiler-profile improvement must not stand in for fixing repeated document scans or parsing. The per-run medians, p95s, build variants and retained logs are in the WP-69 handoff and commit requests.

On the final WP-69 code, the fresh-fixture Firefox run measured Preview median/p95 82.84/86.14 ms against Source 72.92/76.20 ms with the original profile. With the overrides, the same suite measured 73.38/77.76 ms against 68.70/70.66 ms. Both pass the unchanged bound; the overrides are retained solely as a separate development-experience improvement. These are local development-build results, not release or physical-device acceptance.
