---
title: "Lenticulum frontend — requirements"
description: What Moonkale must do as the frontend for Lenticulum.jl, the reason Moonkale was started. Everything the RxInfer, ModelingToolkit and Lux libraries need, and more, derived from three use cases (a language model as a statistical logic program of functions and relations, omics factor graphs, markets with learnable factors). From Daniel, 2026-10-14.
tags: [architecture, julia, lenticulum, requirements, factor-graphs]
---
**From Daniel (2026-10-14):** Lenticulum.jl (MathStruct) was, for him personally, the reason to start Moonkale. Its requirements include everything RxInfer, ModelingToolkit and Lux need, and more. Moonkale is meant to be its frontend. Imagine what one might build with it:
- **huge graphs of the inner workings of LLMs,** by composing diffusion language models and compiled functions;
- **huge graphs for calculating with omics data,** where factors are the actual behaviour of proteins, reagents, etc.;
- **huge graphs of market interactions** with learnable factors.

What exists today: the flow editor with typed ports, undirected pins, subflows and the bipartite rule (ADR-0020 to 0022), symbols and captions (ADR-0024), and three libraries in moonkale-julia (Lux, ModelingToolkit, RxInfer) that generate code. Stage C of ADR-0022 (a subsystem as one factor) and Lenticulum's edge directions are open. [[Julia and Lenticulum]] has the earlier plan (a live graph source, `Moonkale.jl`). This page is the requirement set it is measured against.

## The three use cases, and what each forces

| Use case | Forces |
|---|---|
| **A language model as a statistical logic program** (corrected 2026-10-14, see below): functions and relations compose the model, **not a transformer** | **relations and rules as first-class nodes**, with weights; the **lifted program** drawn once and its **groundings** expanded on demand (the same mechanism as plates); **derivations as proof graphs** with their probabilities ("why did it answer this?"); learnable rule weights; compiled functions as factors whose insides open on demand; denoising or inference steps as a time axis |
| **Omics:** factors are protein behaviour, reagents, assays | factors that **cite their biology** (a UniProt id, a reaction in Reactome, a paper), so the model **meets the knowledge graph**; data bound from tables (measurements, batches, replicates); **units and concentrations** checked across wires; uncertainty everywhere; pathways imported as subgraphs |
| **Markets:** agents and interactions with learnable factors | **time-unrolled** factor graphs (the same slice repeated per step); **learnable parameters** with training runs; streaming data; **scenarios and interventions** (change a factor, compare outcomes) without losing the original |

## The language model as a statistical logic program

**Correction (Daniel, 2026-10-14):** the "LLM introspection" case is not about transformers. Think of a **statistical Prolog**, where **functions and relations compose a language model**. It is something new, being developed. The nearest public relatives, for vocabulary only: probabilistic logic programming (ProbLog, PRISM), Markov logic networks, neurosymbolic languages (DeepProbLog, Scallop), and weighted Datalog. None of them is the target.

What that asks of the frontend:
- **LT-24 Relations, rules and functions as nodes (★):**
  - a relation (a predicate) is a node with its arity and argument types;
  - a rule (a clause) is a factor joining its head to its body literals, carrying a weight or a distribution;
  - a function is a factor with typed inputs and outputs.

  The bipartite shape holds: relations and their arguments are the variables, rules and functions the factors. So the flow editor's model and roles (ADR-0022) are the starting point, not something new.
- **LT-25 Lifted and ground, both visible (★):** the program is authored and drawn **lifted** (rules with logic variables, once). Its **groundings** (the rule instantiated for concrete constants) are generated, counted, and expanded on demand, for one query or one constant. This is LT-03's repetition applied to logic, and what keeps a large program drawable.
- **LT-26 Derivations as proof graphs (★):** a query's answer comes with its **derivations**: which rules fired, on which facts, with what probability each, and how they combine. They are drawn as a proof DAG on the graph and kept as an annotation on the answer ([[North Star]] §7, proofs as annotations; the same view as the knowledge-graph studio's "why?", KG-16). "Why did the model produce this?" is answered by its proof, not by attention maps.
- **LT-27 Queries and their answers:** ask a query (a goal) in the editor, get answers ranked by probability with their derivations, and compare two models or two weight sets on the same query. This uses the logic-query level of the [[North Star]] (§8, the Datalog dialect of WP-20).
- **LT-28 Learning the weights:** rule weights and function parameters are learnable (LT-07), training runs are records (LT-13), and a rule's weight is visible on it (its caption) and traced over a run (LT-12, plots).
- **LT-29 Composition across kinds:** a relation may be defined by rules, by a compiled function, by data (a table: facts bound from a source, LT-09), or by a probabilistic factor. All four kinds compose in one program, and the editor shows which defines what.

## Requirements

`LT-nn`. Several restate what RxInfer, ModelingToolkit or Lux already need. Marked (★) where Lenticulum goes beyond all three.

### Modelling

- **LT-01 Heterogeneous factors in one graph:**
  - probabilistic (RxInfer's distributions);
  - physical and acausal (ModelingToolkit components);
  - neural (Lux layers);
  - compiled functions (any Julia function, later MLIR);
  - data factors (observations);
  - constraints and losses.

  One graph mixes them (★). Each library contributes kinds (`BlockKind`, roles, symbols) as today.
- **LT-02 Composition to any depth:** a subsystem is one factor (ADR-0022 stage C), and subsystems nest. Entering one is a scope change (ADR-0021), and its boundary is its interface.
- **LT-03 Repetition, drawn once (★):** plates, arrays of factors, layers × heads, time slices. The repeated structure is authored and drawn **once, with its multiplicity** (`× 32 layers`, `t = 1…T`), and one instance expands on demand. Never N copies in the document or the scene. This is what makes "huge" tractable.
- **LT-04 Generated graphs (★):** a graph produced by Julia code (a loop, a model constructor) is viewed, navigated and inspected like an authored one. Editing it means editing the generator or overriding an instance, never silently forking the generated output.
- **LT-05 Typed variables:**
  - a variable's space is a domain: real, simplex, positive definite, categorical, a tensor of shape `(B, T, D)`;
  - quantities carry units;
  - a wire between incompatible spaces or units is refused with the reason, like the bipartite rule's refusals today.

  Mycelium's "space" (ADR-0022, not decided) gets decided here.
- **LT-06 Edge directions** (Lenticulum's `Emitting`, `Absorbing`, bidirectional) and per-run polarity: which variables are observed is a run's choice, not the model's.
- **LT-07 Learnable factors:** a factor's parameters carry priors or initial values, are marked learnable or fixed, and have an optimiser per run. A parameter's current value is visible on the factor (its caption, ADR-0024 §6).
- **LT-08 Factors that cite (★):** a factor or a variable links to knowledge nodes (a protein, a reagent lot, a paper, a market instrument) with typed edges. The model graph and the knowledge graph are one graph ([[North Star]] §9). Imported pathways keep their database ids.
- **LT-09 Data binding:** an observed variable binds to a query (SQL over DuckDB or Parquet, a table, a live feed) rather than inline numbers, with its batches and replicates. The binding is part of the run configuration, not the model.

### Running

- **LT-10 A live run:** Moonkale starts or attaches to a Julia process (local, remote, a GPU host through the server) and receives live state: beliefs on variables, messages on edges, energies or free energy on factors, losses. It draws them incrementally on the graph engine.
- **LT-11 Schedules as overlays:** a message-passing schedule is shown as an overlay on the graph (order, sweeps), not as nodes. It is authored as a small separate DAG or as annotations (open question, [[Julia and Lenticulum]]).
- **LT-12 Time:** scrub a run along its schedule steps, training epochs, denoising steps or market time. The graph shows state at the scrubbed time, and plots (LT-17) follow the same cursor.
- **LT-13 Runs are records:** each run keeps its model version (a content hash), parameters, data binding, environment (`Manifest.toml`), seed and outputs. Two runs compare side by side: belief differences on the graph, metrics in plots.
- **LT-14 Interventions and scenarios (★):** clamp a variable, replace a factor, change a parameter, on a **scenario copy** that remembers what it was derived from. Then compare. The `do`-operator for markets and omics is one edit away.
- **LT-15 Checkpoints:** learned parameters are saved per run, and loading a checkpoint fills the factors' captions.

### Seeing it at scale

- **LT-16 Millions of nodes:** the coarse tier ([[Case Selector]], WP-06), aggregation by subsystem and plate (LT-03), level of detail, search and filter by kind, a focus with context. Belief uncertainty is shown as a heat or size on nodes, not as text.
- **LT-17 Plots beside the graph:** loss curves, posterior marginals, time series per variable, and calibration, linked to the graph (select a variable, see its marginal). See [[Plotting Requirements]].
- **LT-18 Inside a compiled factor:** open a compiled function as its IR or MLIR graph ([[North Star]] §3), with provenance back to the source.

### Debugging the model

- **LT-19 Message debugger (★):** step a schedule one message at a time, break on a factor or a condition (a NaN, a belief variance below a threshold, a message that stopped changing), inspect a message's parameters, and see where a divergence first appeared, highlighted on the graph. See [[Debugger Requirements]]: this is a debugger, but for a model, not for code.
- **LT-20 Structural checks before running:** bipartite rule, spaces and units, dimensions, unconnected required channels, cycles where a schedule needs a DAG. These are reported like today's `validate` issues, with the same wording in the panel and in files.

### Files, versions, interop

- **LT-21 The model is a document:** a text form for git and agents, never the UI (ADR-0012). Content-addressed versions, so a run names exactly the model it ran.
- **LT-22 Import and export:** RxInfer `@model`, ModelingToolkit systems and Lux chains import as subgraphs; Lenticulum code generates from the graph, the way the three libraries do today. Round-trips are tested golden-file style.
- **LT-23 Agents:** an agent can build, check, run and explain a model through tools (the same operations as the UI), under the policy gate.

## What exists, what is missing

| Need | Today | Missing |
|---|---|---|
| Kinds, roles, ports, symbols, codegen | flow editor + 3 libraries | Lenticulum's library; edge directions (LT-06) |
| Composition | subflows, flattening (ADR-0021) | subsystem-as-factor (ADR-0022 C) |
| Repetition (LT-03) | — | plates in the flow model and the engine |
| Typed spaces and units (LT-05) | domain-typed pins (`PortType::Named`) | spaces, shapes, units, and refusal by type |
| Live runs (LT-10 to 15) | — | `Moonkale.jl`, the live source, run records |
| Scale (LT-16) | 100 k nodes in the engine | coarse tier, aggregation |
| Plots (LT-17) | — | a plot extension ([[Plotting Requirements]]) |
| Message debugger (LT-19) | — | schedule stepping, conditions, history |
| Knowledge links (LT-08) | wiki-links, sources, the index | typed links from flow blocks to nodes |
| Statistical logic programs (LT-24 to 29) | bipartite roles, subflows, captions | relation and rule kinds; lifted/ground expansion; derivation graphs; a query box over a running program |

## Order (proposal)

Each step is useful on its own:
1. ADR-0022 stage C (WP-08).
2. Plates (LT-03) and typed spaces (LT-05) in the flow model: they decide the document format, so they come before any live work.
3. `Moonkale.jl` and a live run of an RxInfer model (LT-10, LT-12), drawn on today's engine.
4. Plots linked to the graph (LT-17).
5. The message debugger (LT-19).
6. The Lenticulum library itself, with edge directions (WP-09).
7. Scale: the coarse tier and aggregation (WP-06).
8. Knowledge links and data binding (LT-08, LT-09), interventions (LT-14).

Work packages WP-37 (plates and typed spaces) and WP-38 (live runs) in [[Work Packages]]; WP-08, WP-09, WP-06 and WP-10 already cover the rest.
