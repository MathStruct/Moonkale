---
title: "Julia and Lenticulum.jl — the goal behind Moonkale"
description: Why Moonkale exists — a graphical editor for Lenticulum.jl's factor graphs, where a diagram stays a diagram — and how Julia fits without touching the core - the depot as a read-only source, the Lux.jl flow editor, a Simulink-like ModelingToolkit editor, all in a separate repository and never in the standard web build.
tags: [architecture, julia, lenticulum, goals, design]
---
From [[Prompt16]] (2026-09-20). Most people do not use Julia; Daniel does, and **the reason Moonkale exists is [Lenticulum.jl](https://github.com/MathStruct/Lenticulum.jl)** ([theory vault](https://mathstruct.org/Lenticulum.jl/dev/vault/)) — an *implicit* machine-learning library that learns relations instead of functions, whose models are **factor graphs**: bipartite, undirected, cyclic, with named channels whose polarity (input or output) is chosen at use time, run by message-passing schedules until they converge. Such a model is a large graph of interconnections, and the founding premise is:

> [!quote] A diagram stays a diagram
> Something that *is* a diagram is never presented to a human as a declarative text file. JSON is fine — it is a tree of statements, people read trees. A factor graph is not a tree; a page of `connect(...)` declarations is to the model what raw RDF is to the graph behind it. The file on disk is a serialisation for git, diff and agents; the **editor is the interface**.

Nobody has suffered enough with TensorFlow or PyTorch to build a drag-and-drop network editor, because a feed-forward DAG survives as text. A factor graph of a metabolomics model, the current state of a SLAM problem, or a market model does not. That is what Moonkale is for; everything else in this vault is the editor that has to exist around it. Further specification of Lenticulum's editor comes later; this page records the goal and what it already implies.

## Position of Julia in Moonkale
- **Not in the core package.** Everything Julia-specific lives in `MathStruct/moonkale-julia` ([[Extension Catalogue]]) as opt-in extensions; the core never links a Julia toolchain and builds without it.
- **Not in the standard web build.** Julia-WASM exists but is not part of Moonkale's web client; on the web, Julia things run through the server (which may have Julia) exactly as the terminal and LSP do.
- **The flow editor stays core**; Julia libraries (Lux, MTK, Lenticulum) are contributions to it, or — for Lenticulum — a sibling editor sharing its infrastructure.

## 1. The Julia depot as a read-only source
A Julia project is `Project.toml` + `Manifest.toml`; its dependencies live in `~/.julia/packages/<Pkg>/<slug>/` (registry packages, versioned by slug), in `dev` paths (`~/.julia/dev/…` or anywhere, for `Pkg.develop`), and in the stdlib of the running Julia. Wanted: open a folder with a `Project.toml` and Moonkale **adds the resolved dependency tree as a read-only source** — every package's `src/`, `docs/`, `README.md` browsable, indexed (symbols, docstrings, links) and searchable, never editable (the depot is content-addressed; editing it is a bug).

Design: a **`julia-depot` source** (family `Folder`, `capabilities.write = false`, `read_only` shown as the lock — [[Projects and Sources]]) built from `Manifest.toml` (`deps`, `git-tree-sha1`, `version`, `path`, `repo-url`) resolved against `JULIA_DEPOT_PATH` (default `~/.julia`), with one root per package, named `Pkg v1.2.3`. Stdlib comes from the Julia install (`Sys.STDLIB`, found via `julia -e` once, cached). The index treats it like any folder; the graph shows the project's `using` edges into it; search ranks project files above depot files. Desktop and server only (the phone has no depot). Changes to `Manifest.toml` re-resolve the source.

## 2. Editors, from the simplest to the goal

| model | shape | editor | status |
|---|---|---|---|
| **Lux.jl** | a DAG of layers with tensor-shaped ports | the flow editor with the Lux block library — build a network, generate `model.jl` | *exists* (`extensions/lux`, opt-in; running the generated model through the terminal with errors linked to blocks is the open half — [[Flow Editor]]) |
| **ModelingToolkit.jl** | acausal components with physical ports; `connect` is symmetric (no direction), systems nest | a **Simulink-like** editor: component blocks with unit-typed ports, undirected connections, hierarchical subsystems (double-click to enter), parameters and initial conditions in the block, `@named` + `connect` codegen and `structural_simplify` errors linked back to the diagram | planned as the second flow library; needs the flow editor to learn undirected ports and nested subgraphs |
| **Lenticulum.jl / Mycelium.jl** | a **factor graph**: variables (wires with no content) and factors (everything with content — data, priors, losses, optimisers are nodes too), named channels, **polarity chosen when used**, cycles allowed, message-passing schedules, beliefs and energies as state | **the goal**: a graphical factor-graph editor and viewer — see below | not started; specification incoming |

## 3. The Lenticulum editor — what the goal already implies
Two halves that the flow editor does not have and the graph view half has:

**Editing** (structure): variables and factors as two node kinds (bipartite: the editor refuses factor–factor and variable–variable wires); channels as named ports on a factor that carry *no direction* until a polarity is chosen for a run; factor libraries as contributions (LenticulumCore's linear-Gaussian factors, ImplicitLayers, VariationalDiffusion, Adversarial — one library per package, mirroring `lib/`); Lux models wrap as factors (LuxCore) so the Lux library is reused inside a factor; hierarchical grouping (a sub-graph as one factor); a schedule as an overlay, not a node (which messages, in which order); export to Julia as a serialisation nobody has to read, and import from a running Julia session.

**Viewing** (state): the same graph with **live state** — beliefs on variables, energies/free energy on factors, message flow per schedule step — streamed from a Julia process into Moonkale and drawn on the graph engine (the [[Graph View]]: wgpu, Barnes–Hut, and the coarse tier of the [[Case Selector]] for graphs that are large); a time axis to scrub a schedule or a training run. This is the "watching big factor graphs" requirement, and it is a *viewer* problem more than an editor problem: rendering a metabolomics model, the factor graph of a SLAM state, or a market model must be fluent at the sizes those have.

What that needs from Moonkale, concretely — each a piece of existing machinery pointed at Julia:
1. A **companion Julia package** (`Moonkale.jl`, in `moonkale-julia`) that serialises a Mycelium factor graph and streams beliefs/energies over a local socket (JSON lines, later binary) — Moonkale's side is a **live graph source** (`Source` with `WATCH`, `SourceEvent::NodeChanged`) that the graph panel already knows how to redraw incrementally.
2. The flow editor generalised: undirected ports, bipartite constraints, nested subgraphs, port polarity as a per-run property — the MTK editor needs the first three anyway, so MTK is the stepping stone to Lenticulum.
3. The graph view's coarse tier and picking buffer for graphs past 100k nodes ([[Case Selector]]).
4. A document type (`*.lenticulum.json` or the entity log as the store — [[ADR-0012 Two histories]]) whose text form exists for git and agents but is never the UI.

## Not decided
The wire format between Julia and Moonkale (JSON lines first); whether the factor-graph document is a flow-editor document with a stricter schema or its own; how a schedule is authored (overlay vs. a second, small DAG); what "large" is for a factor graph in practice (measure with Lenticulum's own examples once they exist).

Related: [[Core Languages]] (Julia is a core language: grammar, LanguageServer.jl, REPL), [[Extension Catalogue]] (`moonkale-julia`), [[Flow Editor]], [[Graph View]], [[Case Selector]], [[Projects and Sources]] (read-only sources).
