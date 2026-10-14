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

### UC-02: A language model as a statistical logic program
- **Who:** a researcher building a new kind of language model, Daniel's group. Not transformers: a statistical Prolog in which functions and relations compose the model.
- **In Moonkale:**
  1. author relations, weighted rules and functions as a lifted program;
  2. expand groundings for one query;
  3. ask a query and read the answers with their derivations and probabilities;
  4. train the rule weights and watch them move;
  5. compare two weight sets on the same query.
- **Needs:** LT-24 to LT-29, LT-25 (lifted/ground), LT-26 (proof graphs), LT-12/13 (runs), the Datalog dialect (WP-20), the coarse tier.
- **Source:** Daniel, 2026-10-14 (corrected the same day: not transformers).

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

### UC-10: Trading research with Databento
- **Who:** a trader (a friend of Daniel's) using Databento's market-data API.
- **Today:** Databento's clients and other trading tools (to ask him which).
- **In Moonkale:**
  1. pull a day of futures data with its cost shown first, cached;
  2. look at candlesticks and the order-book heatmap;
  3. replay the open;
  4. backtest a strategy as a run and compare parameter sets;
  5. write a journal note that links to the moment on the chart.
- **Needs:** TR-01 to TR-13 ([[Trading with Databento]]), typed cells, plots, run records.
- **Placement:** its own repository.
- **Source:** Daniel, 2026-10-14.

### UC-11: Moving from Obsidian, VS Code or Zed
- **Who:** anyone switching (Daniel; most first-day users).
- **In Moonkale:** open the Obsidian vault as it is (wiki-links, embeds, callouts, canvases); keep VS Code's palette, keybindings and LSP habits; edit references across files in one buffer.
- **Needs:** [[Editor Prior Art (Obsidian, VS Code, Zed)]]'s "taken" rows.
- **Placement:** core and on-by-default extensions.
- **Source:** Daniel, 2026-10-14.

### UC-12: Switch between pieces of work
- **Who:** anyone with more than one thing going on (Daniel: Moonkale, Lenticulum, a paper).
- **In Moonkale:** each piece of work is a session with its own sources, layout, open documents, unsaved buffers, conversations and query history. Switching sessions swaps all of it, and nothing is lost, even unsaved edits. Sessions aren't tied to a folder, and they live in the state store, not in files.
- **Needs:** [[ADR-0025 Sessions are the outermost state]] (WP-46).
- **Placement:** core platform.
- **Source:** Daniel, 2026-10-14.
