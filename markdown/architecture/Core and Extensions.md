---
title: "Core and extensions — a discussion"
description: Where things belong. The library crates, the core platform, extensions in this repository (on or off by default), and extensions in their own repositories. The criteria, today's placement, the open placements for everything requested in October 2026, and the questions to settle by collecting use cases. A discussion document (2026-10-14), not a decision.
tags: [architecture, extensions, discussion, moc]
---
**Why this page:** the requests keep growing. The debugger, plotting, workflows, the knowledge-graph studio, Lenticulum, a static reader, math rendering, canvas components and Unison all came in one week. Each raises "is this core?". The [[Extension Catalogue]] has a short section on core and separate repositories (2026-10-01); this page is the longer discussion, with criteria to apply rather than taste. It is meant to be argued with. Its answers should come from [[Use Cases]], not from preference.

## Four places a thing can live

| Place | What it is | Example today |
|---|---|---|
| **Library** | crates other projects can depend on, versioned by `ext-api` tags, with no Moonkale UI | `moonkale-core`, `moonkale-ext-api`, `moonkale-graph-render` |
| **Core platform** | services every extension builds on, always compiled in: the workspace, sources registry, state store, settings, secrets, policy gate, LSP and terminal services, the shell | the shell, `services/*`, `sources/registry` |
| **Extensions in this repository** | built and tested with the core, toggled per user: *on by default* (what a first-day user needs) or *opt-in* | on: Explorer, Search, code editor, Markdown, Graph, Table, Git, History, Agent, Terminal. Opt-in: Flow editor, the Rust editor and terminal |
| **Extensions in their own repository** | their own release cadence, toolchain or audience; compiled in behind a feature (static) or loaded (wasm) | `moonkale-julia` (Lux, ModelingToolkit, RxInfer) |

## Criteria (proposed)

A thing goes to the **core platform** only if all four hold:
1. **Several extensions need it,** so it is a service, not a feature.
2. It needs **no toolchain** beyond Moonkale itself.
3. Its **dependencies are light** (the core must build in minutes, and on Android).
4. It must behave **the same everywhere,** so one implementation keeps every extension consistent (settings, secrets, the policy gate).

An **extension in this repository** if at least one of these holds:
- a first-day user expects it (then *on by default*);
- it is generic over sources or languages (the flow editor's framework, the table, the graph);
- it shares hot code with the core, so splitting would force lockstep releases.

**Its own repository** if at least one of these holds:
- it needs a foreign toolchain to be useful (Julia, Lean, a JVM);
- it serves a specialised audience;
- it wraps someone else's backend with its own release cadence;
- someone else maintains it.

**The library** gets anything another project would reasonably reuse without Moonkale's UI: the graph engine, the model, the extension contract, perhaps the math parser and the table grid's arithmetic.

## Open placements (proposals to discuss)

| Thing | Proposed place | Why | Doubt |
|---|---|---|---|
| Math delimiters + LaTeX/Typst fallback ([[Math Delimiters and Renderers]]) | the parser in the **library** (shared by rich, source and native preview); rendering in the Markdown extension | three editors must agree on what is math | Typst rendering pulls the `typst` crate: already in the Typst preview, so no new weight |
| Canvas component layer (WP-05) | **library** (beside the engine) | the graph view, flow editor, table schema view and a canvas view all need it | — |
| DAP service ([[Debugger Requirements]]) | **core platform** service like LSP; the debug panel an **opt-in extension** | many language extensions would use one service | is a code debugger wanted at all, beside the model debugger? |
| Model debugger (LT-19) | the **flow editor extension** plus the Julia side in `moonkale-julia` | it debugs flow models | — |
| Plot ([[Plotting Requirements]]) | **extension in this repository, opt-in** at first, on by default once it is good | plotting query results is generic; Lenticulum needs it | the spec format may want to be library-level, shared with reader mode |
| Workflow runner ([[Workflow Automation (n8n, Windmill)]]) | **opt-in extension in this repository** | it runs flows, so it is close to the flow editor and the run records | could grow into its own repository if it gains integrations |
| Run records (LT-13, WF-02) | **core platform** service | Lenticulum runs, workflow runs and debug sessions all need them | — |
| Notebooks ([[Notebook Editor]]) | **opt-in extension in this repository**; kernels behind a feature | generic over languages; KG Studio and Julia need it | — |
| Lenticulum library ([[Lenticulum Frontend Requirements]]) | **`moonkale-julia`** | Julia toolchain, own cadence | Lenticulum is the reason Moonkale exists: should its *editor-side* pieces (plates, typed spaces) be core flow features? Proposed **yes**: they are generic |
| Knowledge Graph Studio ([[Knowledge Graph Studio]]) | **its own repository** | wraps oxilite and tiramemsu, whose cadence is not ours; may be maintained with their author | — |
| Language-server contribution point (KG-05) | **core platform** | every language extension | — |
| Unison source (WP-18), other code-graph sources | **own repository** (`moonkale-sources-extra` or `moonkale-codegraph`) | foreign toolchain (the `ucm` codebase format) | — |
| Datalog dialect (WP-20) | **library** crate | sources, the agent and the KG studio would share it | — |
| Declarative rendering settings (WP-30) | **core platform** (settings schema) + the code view | every language's projection | far away |
| Static reader (WP-28, way later) | its own build in this repository | reuses the engine, grid and index | — |
| Postgres/Supabase source (WP-17) | `moonkale-sources-extra` | needs a server to test | — |

## Questions to settle

1. Is "on by default" decided per extension in the distribution, or per user profile (a "modeller" profile turning on the flow editor and plots)?
2. Should the flow editor become **on by default** once Lenticulum work lands? It's the reason Moonkale exists, but most first-day users won't need it.
3. Does an extension in its own repository get CI here (a check that the distribution builds with it), as `moonkale-julia` does today?
4. Who owns an extension built for someone else's backend (the KG studio): MathStruct, the backend's author, or both?
5. Which services are "core" on Android, where there are no toolchains and no PTY?

**Next step:** collect use cases ([[Use Cases]]), then revisit each "doubt" above against them.
