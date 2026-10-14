---
title: "Work Packages"
description: The open work cut into packages that several agents can carry at once without colliding — each with its files, dependencies, size and acceptance — plus the rules that keep parallel branches mergeable. Drafted 2026-10-14 for a multi-agent setup.
tags: [planning, agents, moc]
---
The [[Roadmap]] is the order and [[Status]] is where things stand. This page cuts the open work into **packages** that run in parallel. The aim is that several agents work at once with little waiting: on each other's branches, on the same hot file, or on a decision only Daniel can make. The direction behind the choice of packages is the [[North Star]].

## Rules for parallel work

1. **One package, one branch, one worktree.** The branch is `wp-NN-<slug>`, in that agent's own worktree (`~/Code/Moonkale/Moonkale-<agent>`) with its own `CARGO_TARGET_DIR`. Never share a target dir or a worktree.
2. **Hot spots have one owner at a time.** These files and crates conflict whenever two branches touch them. A package that needs one claims it in the table below first, and releases it when merged:

   | Hot spot | Why it conflicts | Owner now |
   |---|---|---|
   | `moonkale-core` (`Value`, `Node`, `Query`) | every crate depends on it | — |
   | `ext-api/src/flow.rs` + `ext-api/CHANGELOG.md` | every library and the panel; each change is a tag | — |
   | `graph-render` + the committed wasm in `graph-assets` | binary output; two rebuilds can't merge | — |
   | `Cargo.toml` / `Cargo.lock` (workspace) | every dependency change | merge in order; rebase, don't hand-merge the lock |
   | `markdown/problems/Problem Log.md` | every agent logs | append-only, numbers reserved below |

3. **Numbers are reserved, not guessed.** ADR and P- numbers come from this table. Take the next free one and write your name next to it before using it.

   | Series | Next free | Reserved |
   |---|---|---|
   | ADR | ADR-0025 | ADR-0023 codex (open), ADR-0024 Claude (block symbols, done) |
   | P- | P-195 | master's log ends at P-185; codex's branch holds P-186–P-194 (unmerged). Others start at P-195 |

4. **Done means mergeable:**
   - the gates (fmt, layering, colours, strings, tests, clippy, docs, wasm check);
   - the package's own browser suites, plus the suites of every crate it touched;
   - the docs it changes (the feature's page, [[Status]], [[Extension Catalogue]] when an extension changes);
   - a short HANDOFF note for the reviewer.

   Claude reviews and merges into master. Contributors' branches merge only when Daniel says so.
5. **No idle waiting.** A blocked package names its blocker in its HANDOFF and the agent takes its **side task**: a review of another branch, a doc page, or a test gap in its own area. Questions for Daniel are batched into one message, not asked one by one.
6. **Merge often, small.** A package that runs longer than about three sessions is split. Master moves daily, and each branch merges master before it asks for review.

## The packages

Size: **S** is about one agent session, **M** two to four, **L** many (split before starting). "Touches" lists the crates or areas changed; ★ marks a hot spot.

### Lane A: the code editor (codex)

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 01 | **60 FPS benchmark and baseline** (CED-R22): `editor-fps.mjs` measures frame intervals while typing, holding arrows, scrolling and dragging a selection, in an ordinary file and the 3 MB one | `packages/web/tests/e2e`, docs | — | S | p50/p95/max per interaction recorded in the roadmap |
| 02 | **60 FPS fixes**, profile-led | `code-view`, `editors/code-native` | 01 | M | p95 ≤ 16.7 ms, no frame > 50 ms, on web and desktop release builds |
| 03 | **Rust editor to default** (M3–M5 of the [[Rust Code Editor Roadmap]]): LSP parity, Moonkale context, cutover | `code-view`, `editors/code*`, `services/lsp*` | 02 | L (split per milestone) | each milestone's gate in the roadmap |

### Lane B: the graph engine and canvas

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 04 | **Camera event + embed flag** in the engine: `{kind:"camera", scale, offset}` on pan and zoom, and `InNode.embed` (rect only, no label or symbol) | `graph-render` ★ | — | S | unit tests, and `graph-*` suites pass |
| 05 | **Canvas component layer**: one Dioxus layer over the canvas, one CSS transform per camera change, level-of-detail unmount, click pass-through. The flow editor's parameter editor moves into it and follows pans | new crate `canvas-layer`, `editors/flow` | 04 | M | the planned spec's checklist (specifications README); `flow.mjs` + a new `canvas-layer.mjs` |
| 06 | **Coarse tier**: 1M nodes ([[Case Selector]]) | `graph-render` ★ | — (after 04 merges) | L | a 1M-node fixture stays interactive |
| 07 | **E-graph drawing**: e-classes as groups that collapse to one representative and expand, with rewrite provenance as edges | `graph-render` ★, graph view | 04 | M | a synthetic e-graph fixture suite |

### Lane C: flow editor and Julia (Mistral from about 2026-10-31; Claude until then)

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 08 | **ADR-0022 stage C**: a subsystem as one factor (a role on `Subflow`), RxInfer codegen through it | `ext-api/flow.rs` ★, `editors/flow`, moonkale-julia | — | M | rxinfer e2e with a nested factor, real Julia run |
| 09 | **Lenticulum library**: edge directions, its codegen, a live graph source from a Julia process | moonkale-julia, `ext-api/flow.rs` ★ | 08 | L | a Lenticulum model built, run and drawn |
| 10 | **Run a model from the editor**: Julia process, progress, results back as nodes | moonkale-julia, `editors/flow`, terminal service | 08 | M | mtk/rxinfer "Run" buttons with results in the panel |
| 11 | **Symbols, the rest**: filled regions, grid snapping (ADR-0024 open items) | `editors/flow`, `graph-render` ★ | 04 merged | S | flow suites |

### Lane D: data and tables

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 12 | **Supabase table editor study**: requirements only, no code ([[Scrutiny]] rules). Taken and adapted items go into [[Table Editor]], rejected ones into [[Scrutiny]] | docs only | — | S | both pages updated; Daniel reviews the rejections |
| 13 | **Pushdown**: sort, filter and keyset pagination sent to the source when it can, client-side otherwise (with the "partial" note) | `editors/table`, `sources/sql`, `moonkale-core` ★ (`Query` capabilities) | 12 | M | table-grid suite on a million-row DuckDB file |
| 14 | **Writes to database sources** (R-41) and **inline cell edits** | `moonkale-core` ★, `sources/*`, `editors/table`, policy classifier | 12 | L (split: core write model / SQL / grid editing) | edits round-trip with expected versions; refused ops shown per cell |
| 15 | **Typed cells**: dates, links to graph nodes, vectors as sparklines | `moonkale-core` ★ (`Value`), `editors/table` | — | M | unit and grid tests per type |
| 16 | **Schema view**: tables as nodes with one port per column, foreign keys as wires, using the canvas layer | `editors/table` or a new extension | 05, 12 | M | schema suite on the fixture's SQLite and DuckDB |
| 17 | **Postgres/Supabase source** (stub today) | `sources/sql` | — | M | read-only Postgres in CI (service container) |

### Lane E: the code graph (North Star)

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 18 | **Unison codebase as a graph source**: definitions, types and the dependency DAG, content-addressed (`NodeId::from_content`). The proxy for Sophia | new `sources/unison` | — | M | a Unison codebase opens as a source and draws in the graph view |
| 19 | **Annotations layer** ([[Annotations]]): typed annotations with provenance, shown as engine layers, including **proof** annotations (equivalence, implication, memory safety, safe concurrency) with their checker, the versions checked and stale state ([[North Star]] §7) | `moonkale-core` ★, ext-api, graph view | — | M | annotations on index nodes, toggled as layers |
| 20 | **A Datalog dialect** over graph sources: rules and queries, results lifted into nodes and edges, shown in the table and graph views | new crate, `Query::Text` dialect | 19 helps | M | rule queries over the index fixture |
| 21 | **Persisted, incremental index**: the Recoco/Thread findings ([[External Code Intelligence Projects]], next steps) | `services/index`, state store | — | L (split per acceptance test listed there) | reopening reuses the index; a one-file change touches only its facts |
| 22 | **Calls and references** in the index, for the core languages | `services/index` | — (coordinate with 21) | M | call edges for Rust/Julia/Python fixtures |

### Lane F: platform, security, upkeep

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 23 | **Security issues** #15 (project-fs races, data loss), #18 (SSH), #20 (server batch) | `sources/project-fs`, `services/remote`, `server` | — | M (one per issue) | the issue's repro as a test, closed |
| 24 | **Android**: specs [[028]] (vault export/import, SAF folders) and [[029]] (name and icon from configuration) | `mobile` | — | M | on the S10e (release APK) |
| 25 | **Projects** (spec [[003]], R-40) and cross-source links (R-16) | shell, state store | — | L | per the spec |
| 26 | **CI breadth and flakes**: more browser suites in CI, the `stores` tree-row flake, `live-preview-long`'s noisy bound | `.github`, `packages/web/tests/e2e` | — | S | named suites in CI, three green runs in a row |
| 27 | **Status refresh**: [[Status]] still describes 2026-10-01 | docs | — | S | checked against the code |

### Lane G: publishing

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 29 | **Reader mode**, the server half: read-only sources, visitor caps, deep links | `server`, shell | — | M | a visitor without a token browses and queries a published folder |

**Not scheduled, way later:** WP-28, a stripped-down **static reader** web version ([[Publishing Sources]], "Revisited 2026-10-14"). The full web version stays the web target, and Quartz stays for the vault. No agent picks this up until Daniel schedules it.

## A first wave

Six agents with no shared hot spot:

| Agent | Package | Hot spot held |
|---|---|---|
| codex | WP-01 → 02 | — |
| A | WP-04 → 05 | `graph-render` |
| B | WP-08 (until Mistral is back) | `ext-api/flow.rs` |
| C | WP-12 → 13 | `moonkale-core` (from 13 on) |
| D | WP-18 | — |
| E | WP-23 (#15 first: data loss) | — |

Side tasks for any agent between packages: WP-26, WP-27, and reviews.

**Second wave**, when the first merges:
- WP-16 (needs 05 and 12);
- WP-07 (after 04);
- WP-19 → 20;
- WP-14;
- WP-09 (Mistral);
- WP-21 (split).

## What only Daniel decides

Batched, not blocking. Each package keeps working on its other parts until the answer arrives.

- Which second-wave packages come first.
- Whether WP-14's write model goes through the policy gate the way agent writes do.
- Which Unison codebase to use as the WP-18 fixture.
- The Supabase study's rejections (WP-12), before they are final.
