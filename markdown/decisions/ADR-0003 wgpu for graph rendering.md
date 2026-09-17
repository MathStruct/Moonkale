---
title: "ADR-0003 — wgpu for graph rendering"
tags: [adr]
status: accepted-with-open-question
date: 2026-09-17
---
**Status:** accepted-with-open-question

## Context
The graph view must stay fluid at 100k+ nodes; Obsidian's canvas/SVG approach and Cytoscape degrade well before that. Options surveyed in [[Graph Rendering Options]].

## Decision
Render the graph with **`wgpu`**: instanced draws, GPU compute for force layouts where WebGPU exists, WebGL2 fallback (CPU layout) elsewhere. One renderer; a `SurfaceProvider` trait per platform.

## Consequences
- Web and mobile: straightforward (`<canvas>`).
- Desktop webview: WebGPU availability varies (WebView2 ✅, WKWebView ✅ recent, WebKitGTK ⚠️). **Open question**: run the wasm renderer inside the webview (preferred, one code path) vs. native overlay vs. offscreen streaming. Tracked as [[P-001 Graph surface in desktop webview]]; measure before deciding.
- Custom node renderers from wasm extensions must be declarative (styles, glyph ids) to stay GPU-batched; custom passes are static-only.
- Text rendering on GPU (glyph atlas) is a non-trivial sub-project; reusable by the native terminal and code editor backends later.
