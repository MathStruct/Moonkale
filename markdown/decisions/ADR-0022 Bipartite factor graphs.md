---
title: "ADR-0022 — Bipartite factor graphs: variables and factors as block roles"
tags: [adr, flow, graph, julia, rxinfer, lenticulum]
status: accepted
date: 2026-10-13
---
**Status:** accepted 2026-10-13 (Daniel; written by Claude, covering Mistral's rung while Mistral is out). Related: [[ADR-0020 Undirected pins in the flow model]] (§4 names this rung and leaves "the bipartite rule's error surface, and polarity's representation" to it), [[ADR-0021 Subflows]], [[Julia and Lenticulum]], [[Flow Editor]].

## Context

RxInfer and Lenticulum models are **factor graphs**: bipartite and undirected, cycles allowed. Variables have no behaviour of their own. Factors are everything with content: priors, likelihoods, losses, data. Lenticulum's own model (`lib/Mycelium.jl`, `graph.jl`) settles the shape:

```
VariableNode(id, name, space)        -- a wire; no behaviour
FactorNode(id, name, factor)
Edge(factor, channel, variable, direction, coupling)
```

- An edge joins one named factor **channel** to one **variable**.
- "A channel is a port, not a bus": `connect!` refuses a second edge on the same `(factor, channel)`. Fan-out goes through a variable, and a variable of degree *d* is a copier.
- Polarity (`Observed` / `Unobserved` / `Latent`) is **resolved per message** at run time from which channels carry messages. It is not authored per wire. What *is* authored is the edge's direction (`Emitting`, `Absorbing` or bidirectional) and, per run, which variables are observed (the data).

RxInfer (GraphPPL) has the same bipartite shape: random variables and factor nodes with named interfaces (`out`, `μ`, `σ²`, …), and data variables bound at inference time.

The flow model already has what this needs:
- undirected pins (`BlockKind.connections`, ADR-0020): a pin wire's ends are just its two ends;
- subflows (ADR-0021): "a sub-graph as one factor" later.

What is missing is the rule that a wire joins one variable and one factor.

## Decision

1. **Variables are blocks, not wires.** A flow `Wire` joins exactly two ends, and a variable joins any number of factors. So a variable is a block with one pin of any degree, drawn as a **circle** (factors stay boxes, as on paper). Mycelium's "a variable is a wire" is the semantics; on the canvas, the variable node is that wire's junction.
2. **`BlockKind.role: Option<Role>`, with `Role::{Variable, Factor}`** (`#[serde(default)]`; kinds finished with `..Default::default()` keep compiling). This is a **library-level** property, as ADR-0020 §4 sketched: an RxInfer or Lenticulum library marks its kinds, and every library before it (Lux, ModelingToolkit) has no role and is outside the rule. `BlockPorts.role` carries it through `Flow::ports_of`. A subflow block and a boundary pseudo-block have no role yet; whether a subsystem *is* one factor is stage C's question.
3. **The rule, and where it is enforced.** Between two pin ends that both have a role:
   - **Two of a kind are refused:** "factors connect to variables, not to factors" (and the same for variables).
   - **A factor's channel carries one wire:** "a channel is a port, not a bus". A variable carries any number.
   - Ends without a role are not checked.

   This is the error surface ADR-0020 left open. It has two sites with **one wording**, through the shared `flow::bipartite_refusal` and `flow::channel_refusal`:
   - the panel's wire gesture refuses the wire, so a wrong wire is never stored;
   - `validate` reports what a file brings in (a hand-edited or agent-written `.flow.json`), about the wire or the factor, at the root and inside subflows.
4. **Polarity is not a wire property** (stage B, proposed here, not built). The authored part is per run: a variable is *observed* (bound to data) or not, plus, for Lenticulum, an edge direction where a factor is used one way only. The proposal:
   - a variable block carries its data, and a variable with data is observed, so RxInfer's data variables are simply observed variables. *(Amended 2026-10-13, in stage B: the sketch had a separate `observed` flag, but a flag without data, or data without the flag, would only be two more ways to be wrong.)*
   - Lenticulum's edge direction waits for its library, because Mycelium's `Bidirectional` is the default and the only one RxInfer needs.
5. **Stages.**
   - **A (built, 2026-10-13):** the role, the rule at both sites, variables drawn as circles.
   - **B (built, 2026-10-13, moonkale-julia v0.4.0):** an RxInfer library in `moonkale-julia`: a variable kind, and Beta, Bernoulli, Normal (variance or precision), Gamma and Poisson as factors. `@model` codegen takes the observed variables as the model's data arguments, writes one `~` per factor in graph order (a data vector is i.i.d., in a loop), and an unwired parameter channel takes its constant. Inference is exact message passing, and mean-field variational (with initial marginals and 30 iterations) only when a factor has two latent inputs. An e2e infers a coin's bias: Beta(9, 10), the exact conjugate posterior. The engine learned sized circles (`InNode.width` now sets a circle's radius too), so a variable stays a click target beside the blocks.
   - **C:** a subsystem as one factor (a role on `Subflow`), and Lenticulum's edge directions once its library exists.

## Consequences

- **The change is additive.** `BlockKind` gains a defaulted field. `BlockPorts` gains `role` too: a caller that builds `BlockPorts` literally must add `role: None`. Inside Moonkale only the flow panel does that, and moonkale-julia doesn't. It rides under the CHANGELOG's Unreleased section, and the next `ext-api` tag carries it.
- **Codegen doesn't change for existing libraries.** Lux and ModelingToolkit have no roles, and their files validate as before.
- **Not decided here:** the variable's `space` (Mycelium's type of the wire, so the pin's `PortType` is `Any` for now); schedules (an overlay, not a node, per [[Julia and Lenticulum]]); and whether "observed" belongs to the document or to a run configuration beside it.
