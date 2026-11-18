---
title: "ADR-0032 — Plates and typed spaces in the flow model"
tags: [adr, flow, factor-graph, lenticulum, types, units]
status: proposed
date: 2026-11-18
---
**Status:** proposed 2026-11-18 (Claude, WP-37). Built the same day for the flow model, the flow editor and RxInfer codegen. Related: [[Lenticulum Frontend Requirements]] (LT-03, LT-05), [[ADR-0022 Bipartite factor graphs]] (it left "space" undecided), [[ADR-0021 Subflows]], [[ADR-0020 Undirected pins in the flow model]], spec 031 §4 (group hulls).

## Context

LT-03 asks that repetition be authored and drawn **once, with its multiplicity**: a plate (`i = 1…N`), layers × heads, time slices. Never N copies in the document. LT-05 asks that a variable's **space** (real, positive, a simplex, …) and its **unit** be part of the model, and that a wire between incompatible ones be refused with the reason, like the bipartite rule. ADR-0022 left the variable's space open (its pin was `PortType::Any`).

Until now RxInfer's only repetition was implicit: a variable with several data values became an i.i.d. vector. That cannot express a latent per-instance variable (`z[i] ~ Normal(μ, 1)`, the hierarchical model), and it is invisible on the canvas.

Both change the document format, which is why they come before any live work (requirements §"Order", item 2).

## Decision

### 1. A plate is a named index over a set of blocks, stored once

```json
"plates": [{ "id": "p1", "index": "i", "size": "" , "parent": null }],
"blocks": [{ "id": "y", "kind": "rxinfer/variable", "plate": "p1", … }]
```

- `Flow.plates` and `Subflow.plates` hold the plates of their scope. A plate has an `id` (unique in its scope), an `index` name (`i`, a Julia identifier), a `size` and an optional `parent` plate (nesting: layers × heads).
- **Membership is explicit:** `Block.plate` names the innermost plate the block is in. Not geometry: a plate has no stored rectangle. The canvas draws it as the hull around its members (spec 031 §4), so moving a block never changes the model by accident.
- `size` is text: a positive integer (`"32"`), or empty, meaning "as many as the data" (the length of the observed variables in the plate). A symbolic size (`T`) waits for run configurations (LT-09): the document holds no data-binding names yet.
- All three fields are defaulted and skipped when empty, so every existing flow parses and saves byte-identical.

### 2. What a plate means in a factor graph

A block in plate `i` stands for one copy per index value. In a bipartite factor graph (ADR-0022):
- a **variable** in a plate is a vector `y[i]`;
- a **factor** in a plate is applied once per index value;
- a factor's plates must include every plate of each variable it touches. A factor **outside** a plate cannot take a per-instance variable inside it (which copy?), so that wire is refused: "y is one per i; a factor outside plate i cannot take it — move the factor into the plate". A variable outside, a factor inside, is broadcast (θ shared by every `y[i]`). This is standard plate notation.
- The rule is a refusal like the bipartite rule: `validate` reports it and the wire gesture refuses it with the same words (`plate_refusal`).
- Blocks without a role (Lux, ModelingToolkit) may sit in plates in the document; their code generators refuse plates until they generate them.

### 3. Types: a `value` port type with a space and a unit

`PortType` gains `Value { space, unit }`:
- `space` is one of `real`, `positive`, `unit interval`, `integer`, `count`, `binary`, `simplex(K)`, `posdef(D)`, `categorical(K)` (a dimension may be left out: `simplex`). Stored as that text.
- Spaces are ordered by containment: `binary ⊂ count ⊂ integer ⊂ real`, `binary ⊂ unit interval ⊂ real`, `positive ⊂ real`; the parametric families only meet themselves (`simplex(3)` and `simplex` meet, `simplex(3)` and `simplex(4)` do not). Two types **unify** when one space contains the other (to the smaller); otherwise the wire is refused naming both. `unit interval` and `positive` overlap without containment and are refused: a Beta variable is not a Gamma rate without saying so. Containment, not overlap, is the rule because the refusal must be explainable in one line.
- `unit` is a unit expression (`m`, `m/s`, `kg*m/s^2`, `km`, `1` for dimensionless) over SI base units with prefixes and common derived units (N, J, W, Pa, Hz, V, Ω, g, L, min, h). Units unify when they are equal in dimension and scale. Same dimension at another scale (`km` against `m`) is refused with "convert explicitly": a silent conversion is a silent factor of 1000. An absent unit unifies with any unit (it is not yet known, which is not dimensionless).
- A **unit variable** `'u` (optionally raised: `'u^2`) in a kind's pin type makes the kind unit-polymorphic: Normal's `mean` and `out` are `'u` and its `variance` is `'u^2`. The first wired variable binds `'u` for that block; a later one in another unit is refused ("Normal: mean is in m but out is in s"). Bindings are per block, never shared between blocks.
- `Scalar` unifies with any scalar `Value`; `Any` with everything, as before.

### 4. Where a variable's type comes from

The kind declares the pin's base type; the **block refines it** through two new parameter kinds, `ParamKind::Space { pin }` and `ParamKind::Unit { pin }`. Their values live in `Block.params` as text, like every other parameter, so the parameter card edits them without a new editor and the document gains no new field for them. RxInfer's `variable` kind gets a `space` and a `unit` parameter for its pin `x`; the factor kinds declare their channels' spaces (Beta: `out` in `unit interval`, `a`, `b` positive; Bernoulli `out` binary; Poisson `out` count, …).

A variable has many wires, so it is checked as a whole: its **effective type** is the meet of its own refined type and every attached channel's type (after binding unit variables). An empty meet is an issue on the variable naming the first two wires that disagree, and the wire gesture refuses a new wire that would empty it.

### 5. Drawing

A plate is a group in the engine's scene (spec 031 §4): a tinted rounded hull around its members, nested plates inside their parents. The engine draws a group's **label** at the hull's upper right corner: the multiplicity badge `i = 1…3`, or `i = 1…n` when the size comes from the data. One instance expanding on demand (LT-03's second half) is not built yet; it needs the run's data (LT-09).

### 6. RxInfer codegen

Flattening (ADR-0022 stage C) carries each block's plates (an instance's plates, then the inner ones). Then:
- a plated variable is indexed (`y[i]`, `z[i]`); an observed one's data is its vector; a fixed size must equal the data's length;
- statements of a plate run in `for i in eachindex(y)` (a data-sized plate, over one of its observed variables) or `for i in 1:N`; consecutive statements of one plate share a loop;
- a latent plated variable's posterior prints as `mean.(…)`; mean-field initialisation names the vector (`q(z) = …`, RxInfer initialises each element);
- nested plates, and a data-sized plate with no observed variable, are refused with the reason ("not generated yet" / "give it a size").
- The implicit vector of an unplated variable with several data values stays as it was (old files generate the same code).

## Consequences

- The document format grows by `plates` (flow and subflow) and `Block.plate`, both optional: `Flow::VERSION` stays 1, old files parse and save unchanged. `PortType` and `ParamKind` gain variants: a new `ext-api` tag; libraries that `match` them exhaustively need a `_` arm.
- Units and spaces live in `ext-api` (`flow::units`), with no new dependency.
- Not decided here: symbolic sizes bound to data (with LT-09), instance expansion (with LT-09/LT-13), shapes on `Value` (tensor-valued variables keep `Tensor`), Lenticulum's edge directions (WP-09).
