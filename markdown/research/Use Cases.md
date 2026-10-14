---
title: "Use cases — a collection"
description: Concrete things people want to do with Moonkale, one entry each, so that placement decisions (core or extension), priorities and requirements can be argued from cases rather than taste. Started 2026-10-14; Daniel and anyone else add entries.
tags: [research, use-cases, planning, moc]
---
**Why:** [[Core and Extensions]] decides where things belong, and [[Work Packages]] what comes first. Both should rest on what people actually do. Add a case whenever someone describes a task, a request or a frustration. One case per entry; short is fine.

## Template

```
### UC-nn — <the task in a sentence>
- **Who:** the kind of user (a modeller, a biologist, an app developer, a student …)
- **Today they use:** the tools and the pain
- **In Moonkale:** what they would do, step by step
- **Needs:** the features (link the requirement ids: LT-, KG-, PLT-, DBG-, WF-, …)
- **Placement it argues for:** core, extension here, own repository
- **Source:** who said it, when (or "imagined")
```

## Cases so far

### UC-01: Build and watch a large factor graph (Lenticulum)
- **Who:** a researcher in probabilistic modelling (Daniel).
- **Today:** Julia code (RxInfer, ModelingToolkit, Lux) and plots; the structure lives in the code and in the head.
- **In Moonkale:**
  1. compose the model in the flow editor from library blocks and subsystems;
  2. bind data;
  3. run in Julia;
  4. watch beliefs flow on the graph, scrub the schedule, plot marginals, debug a diverging message.
- **Needs:** LT-01 to LT-23 ([[Lenticulum Frontend Requirements]]), PLT-09, the model debugger.
- **Placement:** the flow editor's generic parts here; Lenticulum's library in `moonkale-julia`.
- **Source:** Daniel, 2026-10-14 (the reason Moonkale exists).

### UC-02: LLM internals as a graph
- **Who:** an ML researcher.
- **In Moonkale:** compose diffusion language models with compiled functions; see layers × heads as repeated structure; open a compiled factor's IR; watch activations during denoising.
- **Needs:** LT-03 (repetition), LT-05 (tensor spaces), LT-18, LT-12; the coarse tier.
- **Source:** Daniel, 2026-10-14 (imagined).

### UC-03: Omics with factors as protein behaviour
- **Who:** a computational biologist.
- **In Moonkale:** factors cite UniProt and Reactome entries; observed variables bind to measurement tables (Parquet in DuckDB); units are checked; posteriors are plotted per protein.
- **Needs:** LT-08, LT-09, LT-05, PLT-01, the table grid, the knowledge graph.
- **Source:** Daniel, 2026-10-14 (imagined).

### UC-04: Market interactions with learnable factors
- **Who:** a quantitative modeller.
- **In Moonkale:** a time-unrolled graph of agents; train the factors' parameters; clamp a variable as an intervention on a scenario copy; compare runs.
- **Needs:** LT-03 (time slices), LT-07, LT-13, LT-14, PLT-09, PLT-12.
- **Source:** Daniel, 2026-10-14 (imagined).

### UC-05: Notes and code in one vault
- **Who:** anyone who uses Obsidian and VS Code side by side (Daniel daily).
- **In Moonkale:** write notes with wiki-links into code symbols; see both in one graph; edit Markdown with formulas in any delimiter style.
- **Needs:** the Markdown editor, the index, the graph view, [[Math Delimiters and Renderers]].
- **Source:** Daniel (the vault itself).

### UC-06: Explore a database
- **Who:** an app developer or analyst.
- **In Moonkale:** open a DuckDB or SQLite file, query, scroll 50 000 rows, plot a column, see foreign keys as a schema graph.
- **Needs:** the table grid, PLT-01, WP-13 (pushdown), WP-16 (schema view).
- **Source:** Daniel, 2026-10-14 (the Supabase-like request).

### UC-07: An RDF store and an agent's layered memory
- **Who:** a knowledge engineer and an agent developer.
- **In Moonkale:** query an oxilite store in SPARQL; see why a triple was inferred; review a tiramemsu memory's conflicts and what it believed last week.
- **Needs:** KG-01 to KG-36 ([[Knowledge Graph Studio]]).
- **Placement:** its own repository.
- **Source:** Daniel, 2026-10-14.

### UC-08: Debug a program
- **Who:** a user who asked for a debugger.
- **In Moonkale:** set a breakpoint, run, inspect variables and the call stack, step.
- **Needs:** DBG-01 to DBG-09 ([[Debugger Requirements]]).
- **Placement:** the DAP service core, the panel opt-in.
- **Source:** a user request, relayed by Daniel, 2026-10-14. *Ask them what they debug (language, local or remote), which decides the first adapter.*

### UC-09: Automate a pipeline
- **Who:** someone running a recurring data job.
- **In Moonkale:** a flow of steps (a SQL query, a Julia script, a plot) runs on a schedule or on file change; each run's step outputs can be inspected.
- **Needs:** WF-01 to WF-08 ([[Workflow Automation (n8n, Windmill)]]).
- **Source:** Daniel, 2026-10-14 (n8n/Windmill as references).
