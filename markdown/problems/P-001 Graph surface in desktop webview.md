---
title: "P-001 — Graph surface in the desktop webview"
tags: [problem, graph, platform]
status: open
phase: 2 → 5
---
**Status:** open · **Related:** [[ADR-0003 wgpu for graph rendering]], [[Graph View]], [[Platform Matrix]]

## Problem
Dioxus desktop renders the UI in a system webview (WebView2 / WKWebView / WebKitGTK). The graph renderer is `wgpu`. A native `wgpu` device cannot draw into the webview's DOM. Options:

| | approach | pros | cons |
|---|---|---|---|
| A | run the **wasm** renderer inside the webview's `<canvas>` (identical to web) | one code path; no IPC | WebGPU support: WebView2 ✅, WKWebView ✅ (recent macOS), **WebKitGTK ⚠️ off by default** → WebGL2 fallback = no compute layouts on Linux |
| B | native `wgpu` **child window / overlay** positioned over the panel | full native perf everywhere | window management per OS; z-order with popups/menus; DPI; input routing; Wayland quirks |
| C | offscreen native `wgpu` → **stream frames** into a canvas | works everywhere | bandwidth/latency; ugly |
| D | move desktop to **`dioxus-native`** (Blitz) | trivially embeds wgpu | 0.8-alpha; no JS → blocked until [[Rust-native Editor Candidates]] land |

## Plan
1. Ship **A** with WebGL2 fallback (Phase 2). Measure: fps and layout time at 10k / 50k / 100k nodes on each webview.
2. Check WebKitGTK WebGPU status at Phase 5 time; if still off, prototype **B** on Linux only.
3. Keep D as the long-term answer.

## Measurements
_(none yet)_ — dev box: Arch, WebKitGTK 2.52.6, NVIDIA RTX 3080 + AMD Raphael iGPU. Probe commands in [[Linux Desktop Setup]].

## Measurements
- 2026-09-18, Arch + WebKitGTK 2.52.6, NVIDIA RTX 3080: the wasm renderer **draws inside the desktop webview** (plan A). Backend not yet recorded (`gl` vs `browserwebgpu` — read it off the Graph panel's info line); no frame-rate numbers yet.

## Decision
_(pending — plan A confirmed viable on Linux; keep the overlay crate as the fallback for machines without WebGL2)_ — proposed strategy in [[ADR-0011 Desktop graph surface strategy]]: runtime probe, plan A when WebGPU exists, native overlay crate `editors/graph-desktop` otherwise.
