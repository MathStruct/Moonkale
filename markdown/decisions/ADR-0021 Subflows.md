---
title: "ADR-0021 — Subflows: a subsystem is one block whose ports the subflow declares"
tags: [adr, flow, graph, julia, mtk, subflows]
status: accepted
date: 2026-10-12
---
**Status:** accepted 2026-10-12 (Daniel; Claude's review folded in — per-instance naming and the ext-api tag). Related: [[ADR-0020 Undirected pins in the flow model]], [[ADR-0018 One frontend graph engine]], [[Flow Editor]], [[Julia and Lenticulum]].

## Context

ModelingToolkit systems nest (`compose`), and the MTK library (ADR-0020's first consumer, in moonkale-julia) proved the pins model end to end — but every circuit is still one flat sheet. The ModelingToolkit editor needs hierarchy, and so does Lenticulum (a sub-graph as one factor). Claude's review order put the MTK library first precisely so this design knows what nesting really needs.

What building the library taught: the panel already owns every shape decision (ADR-0018 §2) — the engine never learned what a "pin" or a "block kind" is; it draws nodes, ports and edges. That is the lever for hierarchy: **the engine needs nothing at all.**

## Decision

1. **One document, nested.** A subflow lives in the same `.flow.json` — `Flow` gains `subflows: Vec<Subflow>` (`#[serde(default)]`, non-breaking like every field since the pins review), where `Subflow { id, name, blocks, wires, inputs, outputs, connections }` reuses the ordinary `Block`/`Wire`/`Port` shapes. *(Amended 2026-10-13, in stage A's implementation: the sketch had one `ports` field, but §2 already says a boundary port is a directed input, an output or a domain-typed pin — the declaration mirrors a `BlockKind`'s three port lists, so the shape carries which.)* A parent block references one by `Block.subflow: Option<String>` (also defaulted). Not a referenced file: the document stays one unit for git, diff, agents and undo — the founding rule ("a diagram stays a diagram"; the file is the serialisation, the editor is the interface).
2. **Boundary ports are declared, and they are the parent block's ports.** `Subflow`'s boundary declaration — `inputs`/`outputs`/`connections`, the same `Port` type a `BlockKind` uses, so a boundary port is a directed input, an output or a domain-typed pin exactly as before. The panel's `to_scene` draws the subsystem block like any other, with its ports resolved from the subflow's declaration instead of a library kind; `wire()` resolves a subflow block's ports the same way. Wires to the subsystem are ordinary wires; nothing about validation's shape changes (the resolution source grows by one case).
3. **Inside, a boundary port is a pseudo-block.** Within the subflow's own `blocks`, a port appears as a block with a reserved kind (`boundary/…`, one per port, placed at the canvas edge and locked there). Inner wires attach to it like any block. The panel owns the illusion; the engine draws a node.
4. **Editing is a scope stack in the panel.** Double-click a subsystem block enters its subflow (the scene swaps — a plain `set_graph`, the same as any document change); a breadcrumb (`parent / sub_1`) and Escape or an empty-canvas click pop out. Undo needs nothing new: a snapshot is the whole document, so an edit inside a subflow restores with the parent intact. The palette, toolbar and status bar serve whichever scope is open; the engine never knows a "level" exists.
5. **No engine change; no group machinery.** The parent view draws the subsystem as **one block** — not a hull, not a group. The engine's groups and hulls stay what they are: graph-exploration devices (spec 031 §4). A subsystem that is closed stays closed; "entering" is a scene swap, not a zoom into a region.
6. **Codegen flattens, with per-instance names.** An MTK subsystem inlines into the parent circuit: its components and nets join the parent's, and each boundary pin's inner net merges with whatever the parent wires to it (a boundary pin is just a net continuation — `connect` never cared where a component came from). Two blocks may reference the same subflow, so every inner component carries a per-instance prefix in MTK's own `₊` style (`rc_1₊resistor_1`) — the second instance must not collide with the first. A Lux subflow is a sub-`Chain`: the inner layer path splices between the boundary input and output. Nothing generates a nested Julia structure; `structural_simplify` flattens anyway.

## Stages

- **A. Model + parent view:** `Subflow`, the `Block.subflow` reference, boundary ports on the parent block, validation of the boundary wiring. Native tests; a subflow block visible with its declared pins in the browser.
- **B. Editing:** the scope stack, boundary pseudo-blocks inside, undo across levels, the breadcrumb. The palette can *create* a subflow (a "Subsystem" block whose empty subflow you enter and fill).
- **C. Codegen + e2e:** MTK flattening and the Lux splice in moonkale-julia (its v0.3.0, the same tag ladder); `mtk.mjs` grows an RC whose RC-filter is one subsystem — golden generation and the same 99.3 % simulation, proving the flatten merges nets correctly.

## Consequences

- The document schema grows two defaulted fields — old **files** parse; source compatibility is a separate question: `Block.subflow` is a new field on `Block`, and `Block { … }` literals that list every field break. Stage A therefore ships with a new `ext-api-YYYY-MM-DD` tag and its CHANGELOG entry, like the pins did (`Block: Default` covers only literals finished with `..Default::default()`).
- Subflows compose: a subflow may reference a subflow (the scope stack is a stack; flattening recurses). Depth is bounded by sense, not by the model.
- What we do **not** decide here: cross-subflow references between documents (Lenticulum's "a sub-graph as one factor" may want a referenced subflow later — the model leaves room: `Block.subflow` names an id, today always in the same document), and the palette/UX of editing boundary declarations (a small editor on the subsystem block, like the parameter editor).
