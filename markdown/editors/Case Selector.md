---
title: "Case selector — the right backend for the size at hand"
description: Editor and graph pick their backend by measured size — CodeMirror with the full display for normal files, a plain GPU-friendly text view for stupidly big ones; the CPU layout for normal graphs, a coarsened/precomputed or GPU-compute path past a hundred thousand nodes — with thresholds, hysteresis, a user override and an indicator. Design, with what already exists.
tags: [editors, graph, performance, design]
---
From [[Prompt13]] (2026-09-19). Zed's argument: one editor cannot be both rich and fluent on a 2 GB log; pick per case. Moonkale already has the seams for this — `CodeEditorBackend` behind a trait ([[JS Interop Boundary]]) and a graph pipeline with size-dependent switches — so the **case selector** is a rule that chooses among backends from measured properties, plus a user override and an indicator saying which one is active.

## The selector itself
- **Inputs are measured, never guessed**: bytes, line count, longest line, whether the file is binary/minified (for text); node and edge count, whether a GPU/WebGPU/WebGL2 is present, platform (phone vs desktop), whether a precomputed layout exists (for graphs).
- **Tiers, with hysteresis**: a document that grows across a threshold while open does not flip backends mid-edit; the tier is chosen at open (and at *Reload*), and the status bar says so (`plain · 1.2 GB` / `graph · coarse · 1.4 M`). Switching costs a reload, so it is explicit.
- **Override**: *View → Open as plain text* / *Open with rich editor* on the tab's context menu and as palette commands (`editor.tier.plain`, `editor.tier.rich`, `graph.tier.*`); the choice is remembered per file in the workspace settings.
- **Thresholds are settings** (`editor.tier.*`, `graph.tier.*`) with the defaults below; the defaults are what the measurements support, not round numbers picked for looks.

## Text: three tiers

| tier | when (defaults) | backend | what you get | status |
|---|---|---|---|---|
| **rich** | < 4 MB **and** < 100 000 lines **and** longest line < 10 000 chars | CodeMirror (`backend::codemirror`) | everything: highlighting (when it exists), LSP, folding, search panel, completion, presence marks, rich markdown | *exists* — the only backend today, used for every size |
| **plain** | up to ~1 GB, or any file with a very long line (minified JS, JSONL, logs) | `backend::plain` — a virtualised Dioxus text view over the same rope: only the visible rows are in the DOM, drawn with a monospace grid; no decorations beyond search hits and the cursor; optional read-only | fluent scrolling and editing on big files; search; go-to-line; LSP off | **new**; the trait is there, the view is the work ([[Rust-native Editor Candidates]] names it as the eventual CodeMirror replacement — the plain tier is its first, feature-poor incarnation) |
| **viewer** | > ~1 GB, or when the rope would not fit memory | memory-mapped, paged, read-only; line index built lazily; search streams | look, search, copy | new; small |

Why the rich tier stops at 4 MB: CodeMirror itself is fine well past that, but our channel is not — the JS bridge still sends **the whole text per keystroke** (P-037), which is O(n) per key; and the LSP/index work per change is proportional to the file. The plain tier requires fixing P-037 (splices, not snapshots) — that fix also raises the rich threshold, so it comes first. On the phone the rich threshold is lower (`1 MB`); the WebView's memory is the limit there.

"GPU support" for text: the plain tier's row grid is what a wgpu text renderer would draw when the desktop moves to `dioxus-native` ([[JavaScript Inventory]]); in a webview, a virtualised DOM grid is the equivalent and is fluent to millions of lines because only ~100 rows exist at a time. The GPU path is a renderer swap under the same tier, not a fourth tier.

## Graph: three tiers

| tier | when (defaults) | layout | rendering | status |
|---|---|---|---|---|
| **exact** | < 1 500 nodes | Fruchterman–Reingold, exact O(n²), settles in < 1 s | wgpu instanced, labels on | *exists* (`BARNES_HUT_FROM = 1500`) |
| **approximate** | 1 500 – 100 000 nodes | Barnes–Hut quadtree (θ = 0.8), iteration caps, fewer steps per frame | same; labels off above 20 000 (`web.rs` LOD); CPU hit test | *exists*; measured 100k at 169 ms/step native, 189 in wasm — a 100k graph settles in ~17–20 s ([[Milestone 6 - Implementation Log]]); the panel asks sources for at most `GRAPH_LIMIT = 100 000` |
| **coarse** | > 100 000 nodes, up to millions | **not a bigger layout — a smaller graph**: the source returns a *coarsened* graph (folders instead of files, modules instead of symbols, communities instead of vertices; ≤ 100k nodes at any level) with a precomputed or cached layout, and the user expands a region into the approximate tier | instanced as now, plus a **picking buffer** (GPU id pass) because the CPU hit test is O(n); edges drawn only for the expanded region | **new** |

Why coarsening, not "GPU layout" first: extrapolating Barnes–Hut, 1 M nodes is ≈ 2 s per step and ≥ 120 steps — minutes, on any CPU. A WebGPU compute layout (an n-body kernel or GPU Barnes–Hut) would be 50–100× faster per step, but it does not run on WebGL2 (Firefox, the Android WebView, the desktop's GL path — P-089) and it does not solve the other two walls: **transfer** (1 M nodes as JSON is ≈ 100 MB through `eval`; the coarse tier needs a binary path — typed arrays over the wasm memory, or a fetch of a compact buffer) and **legibility** (1 M points is a hairball; nobody reads it). Coarsening removes all three; GPU compute is then an optional accelerator *inside* the approximate tier (`graph.layout = "gpu"` when WebGPU is real), which is where [[Graph View]] already puts it ("only if Barnes–Hut fell short").

Sparsification in the coarse tier, concretely: (1) hierarchy-based — the index already has `Contains` and `Defines` edges, so folder/module nodes are free; (2) degree-based — keep hubs and the k-nearest neighbourhood of the selection; (3) community-based (Louvain/Leiden on the server or natively, cached in `.moonkale/`) for sources without a hierarchy (databases, graph DBs). The layout of a coarse level is computed once (server/native, off the UI thread) and cached with the index; expanding a region lays out only that region, pinned around its coarse node.

## What exists, what is missing
- *Exists*: the trait for text backends; the Barnes–Hut switch, label LOD, iteration caps and the source limit for graphs; measured numbers to set thresholds from.
- *Missing*: the selector (measure at open, choose, indicate, override, remember); `backend::plain` and the viewer; P-037 splices; the coarsened graph queries in the index (`Query::Coarse { level }`), the binary transfer path, the picking buffer, region expansion; optional WebGPU compute layout.
- Verification when built: E2E opens a generated 50 MB file and scrolls/edits fluently in the plain tier (frame time asserted via `performance.now()`), and a synthetic 1 M-node source shows a coarse graph in < 2 s with a region that expands.

## Order (when scheduled)
1. P-037 splices (raises the rich threshold, prerequisite for plain).
2. Text selector + `backend::plain` + indicator/override.
3. Graph coarse tier from the hierarchy the index already has + picking buffer + binary transfer.
4. Viewer tier; community coarsening; GPU compute as an accelerator.
