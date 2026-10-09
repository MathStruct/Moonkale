---
title: "ADR-0018 — One frontend graph engine: the flow editor moves onto graph-render"
tags: [adr, graph, flow, renderer]
status: accepted
date: 2026-10-08
---
**Status:** accepted 2026-10-08 (Daniel's decision, given with spec [[031]]). Related: [[ADR-0011 Desktop graph surface strategy]], [[ADR-0016 Freya is a reference, not a target]], [[research/dioxus-flow]], [[Flow Editor]], [[Frontend Graph Engine Mapping]].

## Context
The flow editor's canvas is `dioxus-flow` 0.1.2 — a third-party crate (~8.3k lines, one author, crates.io) adopted in Milestone 6 after [[research/dioxus-flow]] recommended it for drag-and-drop flows at ~1 000 nodes, with the wgpu graph view kept as a separate surface. Spec [[031]] now asks for the opposite end state: graph exploration and flow/canvas editing as distinct interaction modes over **one** renderer (§5), with ports as independent hit targets, orthogonal routes, layers, LOD discipline and 100k-scale behaviour (§2–§6).

- **We do not own `dioxus-flow`.** The substantial changes the spec needs cannot be contributed upstream at our pace, and a fork of a 0.1.x DOM/SVG canvas buys none of the instanced-WebGL scale that §6 exists for. Its DOM rendering caps it near a thousand nodes (its own roadmap; the flow editor is fine today, but spec 031's domains — factor graphs, hardware layouts — are not).
- **Only the canvas depends on it.** The flow model already lives frontend-free in `ext-api::flow` (`FlowLibrary`, `PortType`/`unify`, `validate`, codegen); `panel.rs` converts between it and `dioxus_flow::{Node, Edge}` at every sync, so the renderer swap is confined to that conversion.
- **Lenticulum.jl and ModelingToolkit.jl integration must stay mostly frontend-agnostic** (Daniel): block libraries, port types, validation and codegen are extensions' contributions over `ext-api::flow`, whatever draws them — the same headless-model rule as ADR-0016's editor.

## Decision
1. **One engine.** `moonkale-graph-render` grows an edit mode (spec 031 §5: ports, typed wiring, orthogonal routes) and the flow editor moves onto it. `dioxus-flow` is gradually replaced by our own flow editor extension, per stage 7 of [[Frontend Graph Engine Mapping]], gated on a parity checklist (ports, typed validation hooks, layered layout, undo, parameter editing).
2. **The model stays frontend-agnostic.** `ext-api::flow` keeps no renderer types; Lenticulum/MTK libraries never learn which canvas drew them. Validation and simulation stay outside the renderer, as the spec demands.
3. **No fork of `dioxus-flow` and no upstream work on it.** It stays until stage 7 retires it; [[research/dioxus-flow]] remains the record of why it was the right choice at Milestone 6 scale.
