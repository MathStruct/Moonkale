---
title: "Graph Rendering Options"
tags: [research, graph]
---
Requirement: fluid at 100k+ nodes, popups, coloured/directed edges, subgraphs, 3D. Decision: [[ADR-0003 wgpu for graph rendering]].

| Option | Scale ceiling (interactive) | Compute layouts | Platforms | Verdict |
|---|---|---|---|---|
| SVG / DOM (Obsidian-style, Cytoscape default) | ~2–5k | CPU | all | ❌ the problem we're solving |
| Canvas 2D | ~10–20k | CPU | all | ❌ no batching, no compute |
| WebGL2 via `wgpu` | ~100k draw, layout CPU-bound past ~10k | ❌ (no compute) | all webviews | ✅ **fallback** |
| WebGPU via `wgpu` | 100k–1M | ✅ compute | Chromium, Safari (recent), WebView2, WKWebView; WebKitGTK ⚠️ | ✅ **primary** |
| Native `wgpu` window (overlay over webview) | 1M+ | ✅ | desktop only | ⚠️ plan B for Linux |
| `dioxus-native` (Blitz) with custom wgpu paint | 1M+ | ✅ | desktop | ⚠️ 0.8-alpha; no JS ⇒ blocked by [[JS Interop Boundary]] |
| `egui_graphs` | ~10k | CPU | egui contexts | ❌ different UI stack |
| Sigma.js / Cosmos (JS, WebGL) | 100k+ | Cosmos: GPU | web | ⚠️ works, but violates Rust-first; keep as reference for shader techniques |
| Three.js / 3d-force-graph (JS) | 50k | CPU | web | ❌ |

## Techniques to adopt (regardless of surface)
- Instanced rendering: one draw per pass.
- SDF glyphs for nodes; glyph atlas for labels; label LOD by zoom.
- Pick buffer (render ids) instead of CPU hit-testing.
- Force layout on GPU: grid-binned repulsion (simpler than Barnes-Hut on GPU; good to ~500k).
- Edge bundling and importance-thinning beyond N edges.
- Persist positions per saved view.

## 3D
Same instance buffers; perspective camera; z from a third layout dimension or a property (time/layer); depth-sorted transparent edges; fog. Cost is mostly UX (orbit controls, labels facing camera), not rendering.
