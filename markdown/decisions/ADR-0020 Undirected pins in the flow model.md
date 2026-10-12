---
title: "ADR-0020 — Undirected pins in the flow model: the ladder toward ModelingToolkit"
tags: [adr, flow, graph, julia, mtk]
status: proposed
date: 2026-10-12
---
**Status:** proposed 2026-10-12 (Daniel's direction: Lux.jl → ModelingToolkit.jl → RxInfer.jl → Lenticulum.jl, "so the graph engine displays actual projects"). Related: [[ADR-0018 One frontend graph engine]], [[Flow Editor]], [[Julia and Lenticulum]], [[Frontend Graph Engine Mapping]].

## Context

The flow editor models a **directed** dataflow: `BlockKind` has `inputs` and `outputs`, a `Wire` goes from an output to an input, `validate` type-checks that direction, refuses unconnected required inputs and reports cycles, and codegen walks the topological order. That is Lux.jl's shape (a DAG of layers).

The next rung of the ladder is **ModelingToolkit.jl** — acausal components whose `connect` is symmetric (no direction), systems nest, and loops are normal (algebraic loops, feedback) — and beyond it RxInfer/Lenticulum's factor graphs, which are also undirected and bipartite. [[Julia and Lenticulum]] names exactly this: "undirected ports, bipartite constraints and nested subgraphs are what the ModelingToolkit editor needs".

The engine side is already in place: `InPort.direction` has a `Both` variant (its default), ports draw by colour, the wire gesture starts from any port, and `InEdge.directed = false` draws no arrowhead. What is missing is the model and the panel.

## Decision

1. **`BlockKind` gains `connections: Vec<Port>`** — undirected pins, `#[serde(default)]`, so every existing flow and library parses unchanged. A pin's `PortType` names the **domain** both ends must share: `Named { name: "electrical" }`, "mechanical", … — `unify` already requires name equality for `Named`, which is exactly MTK's domain rule. A directed library (Lux) simply has no pins.
2. **A wire between two pins is an equation, not dataflow.** It is accepted in either drag order; a pin may carry **several wires** (a bus — `connect(p, n, q)`); it is **exempt from the cycle check** (`Flow::topological_skipping` ignores pin wires; `topological()` itself keeps its meaning — the directed dataflow order the Lux codegen walks); and its type check is symmetric (`unify` both ways). Required pins must be wired in at least one direction.
3. **The panel draws pins on the block's top edge** with `direction: "both"` — the engine's own default — and pins' wires as `directed: false` edges. Port names must be unique across a kind's inputs, outputs and connections; the panel resolves a wire by trying the directed interpretation first, then the pin one.
4. **Staging toward the goal.** This ADR covers the pin half; the rungs after it, in order:
   - **Nested subflows** (double-click into a subsystem, the parent drawing it as one block with boundary ports — the engine's group machinery is the basis) — its own design pass before implementation;
   - **the ModelingToolkit library** in [`MathStruct/moonkale-julia`](https://github.com/MathStruct/moonkale-julia): components with unit parameters, `@named` + `connect` codegen, `structural_simplify` as the run hint, and an e2e that builds a real RC circuit;
   - **RxInfer**: bipartite constraints (a library-level rule: variable and factor kinds refuse same-kind wires) and per-run channel polarity;
   - **Lenticulum**: the factor-graph editor and its live half — a `Moonkale.jl` companion streaming beliefs and energies into a live graph source.
   Alongside: the Julia depot source (Project.toml/Manifest.toml as a read-only source with `using` edges) so the graph engine shows real code projects.

## Consequences

- `.flow.json` stays the document; a pin wire is the same `Wire` shape (its `from`/`to` are just the two ends; codegen decides what a wire *means* — layers or `connect` equations — by looking at the ports).
- The Lux codegen (moonkale-julia) is untouched: it walks `topological()`, which never sees pins.
- The engine needed no change for this stage; the acausal drawing rules (pins on the top edge) are the panel's conversion, like every other shape decision (ADR-0018 §2).
- What we deliberately do **not** decide here: subflow document structure, the bipartite rule's error surface, and polarity's representation — each gets its own pass at the rung that needs it.
