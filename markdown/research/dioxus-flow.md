---
title: "dioxus-flow — evaluation"
description: A react-flow-style node graph library for Dioxus, evaluated for the Flow Editor.
tags: [research, flow]
---
**Crate:** [`dioxus-flow`](https://crates.io/crates/dioxus-flow) 0.1.2 · [repo](https://github.com/XiangpengHao/dioxus-flow) · same author as `dioxus-workbench`, which we already depend on. ~8.3k lines, deps: `dioxus`, `serde` (optional), `web-time`; no JS.

## What it offers (from README + source, 2026-09-17)
Pannable/zoomable canvas, draggable nodes, **connectable handles** (drag to create an edge, snapping), bezier/straight/smooth-step edges with labels, arrowheads, animated edges, **layered auto-layout** in four directions with animation, "seat anchoring" for crossing-free endpoints, minimap/controls/background, custom node and edge rendering with any Dioxus component, keyboard/ARIA, dark mode via CSS variables, 60 fps at ~1000 nodes. Roadmap: box selection, edge reconnection, sub-flows/grouping, viewport culling.

## Fit for Moonkale
| need ([[Flow Editor]]) | dioxus-flow | verdict |
|---|---|---|
| drag-and-drop blocks wired by typed ports | handles + connections + custom node views | ✅ direct hit |
| validation (port types, cycles) | not built in; we validate on the `connect` event and reject/colour the edge | ✅ our layer |
| storage as ordinary nodes/edges (`Block`, `flow.wire`) | nodes/edges are plain signals we own | ✅ maps 1:1 onto `core` |
| layered layout for DAGs (ML models, MTK) | built-in, animated | ✅ replaces our own `hierarchical` for flows |
| very large flows | DOM-based, ~1000 nodes | ⚠️ fine for flows; the wgpu graph view stays for the knowledge graph |
| Rust-only, no JS | yes | ✅ no interop boundary needed |
| maturity | 0.1.x, one author, CI | ⚠️ same risk profile as `dioxus-workbench`, which worked out |

## Recommendation
Adopt `dioxus-flow` as the **canvas of the Flow Editor** (Phase 5, P-24): it removes the SVG canvas, wiring, snapping and layout work from `packages/editors/flow` and leaves us the parts that are actually ours — block libraries as extension contributions, port-type validation, and codegen (Lux.jl / ModelingToolkit.jl). Keep the graph view (wgpu) separate; the two crates address different scales and interaction models. One design implication for now: `editors/flow` should model blocks/ports so they convert to `dioxus_flow::{Node, Edge}` without a second representation.

Not adopted yet — nothing in Milestone 2 needs it. Recorded in [[Flow Editor]].
