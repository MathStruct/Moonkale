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
   | ADR | ADR-0027 | ADR-0023 codex (open), ADR-0024 Claude (block symbols, done), ADR-0025 Claude (sessions, accepted), ADR-0026 Claude (memory and the store, accepted) |
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

Later in lane E, after a structure editor exists:

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 30 | **Declarative rendering settings** ([[North Star]], "Proposal: declarative rendering settings"): a `[render]` settings schema (keywords, index base, blocks, type annotations, macros, operators), and projections over typed nodes that parse back. First as a design note with the invertibility rules and the index-base case worked out; code only after WP-18/19 give typed nodes | settings schema, a projection crate, the code view | 18, 19; a structure editor | L (design first: S) | design: the schema, the invertibility rules, and three worked examples (`fn`/`function`, 0/1-based index, collapsed macros) reviewed by Daniel |

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

### Lane H: knowledge graphs (RDF and layered memory), proposed

The frontend for oxilite and tiramemsu, using their crates as they are ([[Knowledge Graph Studio]]). It lives in its own repository, like moonkale-julia. Not scheduled.

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 31 | **An oxilite store as a source**: SPARQL in the table, CONSTRUCT/DESCRIBE in the graph view, read-only (KG-01, 08, 09) | new extension repo; `ext-api` source contract | — | M | a demo store queried and drawn in the browser suite |
| 32 | **A language-server contribution point**, then `oxilite studio-server` for SPARQL, Turtle, Datalog and Cypher; run, explain, pins (KG-03, 05, 06) | `services/lsp-local` (discovery), ext-api ★ | 31 | M | completion from the store's vocabulary in a pinned `.rq` |
| 33 | **Trust**: resource view, inferred badges, "why?" proof trees as annotations, live SHACL diagnostics (KG-12, 15–17) | the extension; annotations (WP-19) helps | 31 | M | the demo's deliberate SHACL violation shows at its line, with its proof |
| 34 | **Schema registry** view and edits through oxilite's registry API (KG-19–22) | the extension, graph view styles | 31 | M | the zoo/garden/common scenario from the spec |
| 35 | **Layered memory**: tiramemsu as a source, statement junctions, the two-clock control, conflicts, a fact's life, saved answers (KG-27–36) | the extension, `graph-render` ★ (junction styling) | 31 | L (split) | tiramemsu's README questions answered in the UI |

### Lane I: requests of 2026-10-14

| WP | What | Touches | Depends on | Size | Done when |
|---|---|---|---|---|---|
| 36 | **Math: any delimiter, LaTeX or Typst per formula, a normaliser** ([[Math Delimiters and Renderers]]) | a shared math parser (library), `editors/markdown`, `packages/js/milkdown`; the native preview (codex) | — | M | the acceptance list in the spec |
| 37 | **Plates and typed spaces in the flow model** (LT-03, LT-05): repetition drawn once with its multiplicity; variable spaces, shapes, units; refusal by type | `ext-api/flow.rs` ★, `editors/flow`, `graph-render` ★ (multiplicity badge) | 08 | M | an RxInfer i.i.d. plate and a unit mismatch refused, in the browser |
| 38 | **Live runs** (LT-10, 12, 13): `Moonkale.jl`, a live source, run records (a core service, shared with WF-02) | moonkale-julia, a new run-record service, `editors/flow` | 08 | L (split: run records / Julia side / drawing) | an RxInfer run streams beliefs onto the graph and scrubs |
| 39 | **Debugger**: a DAP service and an opt-in debug panel ([[Debugger Requirements]]), after the model debugger | new `services/dap`, a new extension, code editor decorations | 38 (model debugger first) | L | breakpoint, step and inspect in Rust and Julia |
| 40 | **Plot extension** ([[Plotting Requirements]]): the quick look first (PLT-01 to 03, 06), then live plots (PLT-09) | a new extension, `editors/table` (Plot action) | — | M | a table result plots in one action; a selected point selects its row |
| 41 | **Workflow runner** ([[Workflow Automation (n8n, Windmill)]]): run records and re-run from a step first | the run-record service (38), `editors/flow` | 38 | L | a three-step flow runs on file change, and each step's output is inspectable |
| 42 | **Databento market data** ([[Trading with Databento]]): the source with cost estimates and a local cache, typed schemas, symbology; then order-book views and replay | its own repository; WP-15 (typed cells), WP-40 (plots), WP-38 (runs) | 15, 40 | L (split) | a cached day of CME futures in the grid and as candlesticks, replayed |
| 43 | **Multibuffers over nodes** ([[Editor Prior Art (Obsidian, VS Code, Zed)]]): one editable buffer of excerpts from files, rows and code-graph definitions | `code-view`, the code editor (codex's lane) | 03 | L | references to a symbol edited in place across three files |
| 44 | **JSON Canvas** read and write on the graph engine with canvas components | the canvas layer (WP-05), a new view | 05 | M | an Obsidian `.canvas` file opens, edits and saves back unchanged where untouched |
| 45 | **Statistical logic programs in the flow model** (LT-24 to LT-29): relation and rule kinds, lifted/ground expansion, derivation graphs, a query box | `ext-api/flow.rs` ★, `editors/flow`, the Datalog dialect (WP-20) | 37, 20 | L | a small weighted program answers a query with its proof graph |
| 46 | **Sessions** ([[ADR-0025 Sessions are the outermost state]], accepted): session tables in the state store, the switcher, migrating per-folder rows into the Default session, export and import | `state`, shell, `editors/agent` (conversations), `moonkale-core` ★ (settings scopes) | — | M | switch two sessions with different sources and an unsaved buffer each; nothing lost |
| 47 | **Security hygiene in CI** ([[Security]] rules 4 and 6): `cargo deny` and `cargo audit`, JS lockfile audit, the first fuzz targets (math delimiters, the flow format) | `.github`, `deny.toml`, new fuzz crates | — | S | CI fails on a new advisory |
| 48 | **Participants, roles and agent tasks** ([[Shared Work]] SW-01 to 08): roles per session, agents with sponsors, a task queue, an approval inbox, attribution, budgets | server (accounts first), shell, `editors/agent`, the entity log | accounts on the server; 46 | L (split) | two people and an agent in one session, the agent's writes approved from the inbox |
| 49 | **Proposals and optimistic writes** (SW-20, 21, 23): versioned writes through `Source`, a conflict view, changesets reviewed and applied atomically | `moonkale-core` ★ (write API, R-41), `sources/*`, a review view | — | L | an agent's two-file edit and a SQL update applied as one reviewed proposal; a stale write refused and shown |
| 50 | **Comments** ([[Annotations]] steps + SW-30, 31, 34): threads on text, nodes, flow blocks, cells; rich rendering; `@agent` mentions later | `moonkale-core` (Annotation kind), ext-api, editors, the state store | — | M | a comment on a flow block and one on a table cell, both found in the Annotations panel |
| 51 | **Git sync and conflicts** ([[Version Control Requirements]] VC-06, 07): fetch, pull, push (with lease), credentials from the system; the three-way conflict view (shared with SW-21) | `extensions/git` | — | M | a rebase conflict resolved in the view and pushed |
| 52 | **Git everyday, the rest** (VC-03, 05, 09, 10, 11): hunks and lines, branches, blame, reflog recovery, stash | `extensions/git` | — | M | each in the git suite |
| 53 | **A version-control trait, semantic diffs, then Jujutsu** (VC-20, 21, 23): history of a node; flow and table diffs; jj as the second system | `moonkale-core` ★ (the trait), `extensions/git`, `editors/flow`, `editors/table` | 51 | L | a flow diff drawn on the canvas; the same views on a jj repository |
| 54 | **Tools as actors** ([[Shared Work]] §5, SW-40 to 45): provenance on derived data, derived marked and stale, tool writes as proposals | `moonkale-core` ★ (provenance), the index, LSP, the flow libraries' codegen | 21 helps | L | a parser's symbols marked stale after an edit and fresh after the reparse; a codegen run reviewed as a proposal |
| 55 | **The benchmark runner** ([[Benchmarks]]): the bench lock, the results database, the existing benchmarks moved in, then the frame gates and the no-growth test | `packages/web/tests/e2e`, a small runner | — | M | BM-04, 09, 11 and 25 recorded in the results database on a quiet machine |
| 56 | **Memory and the store** ([[ADR-0026 Memory and the store]], accepted): recovery snapshots of unsaved buffers, derived data keyed by input hash, cache budgets per platform | `state`, `moonkale-core`, the index | 21 | M | a killed app restores an unsaved buffer; reopening reuses the index |
| 57 | **Crash safety, the rest** ([[Crash Safety]] steps 3 to 7): panics in tasks and handlers caught per extension, a web crash overlay, crash counting and safe mode, re-instantiating wasm modules | shell, `ext-host`, the web client | — | M | a panicking task disables only its extension; `--safe` starts clean |

## A first wave

How to run it (assignments, prompts, and what collides on one machine) is [[Running Several Agents]].

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

Only fundamental decisions ([[Running Several Agents]], "Decisions while agents run"); the coordinator decides the rest. Batched, not blocking. Each package keeps working on its other parts until the answer arrives.

**Open fundamental questions:** none at the moment.

**Decided by the coordinator (2026-10-14), formerly listed here:**
- **The second wave's order:** WP-46 sessions, then WP-50 comments, then WP-55 benchmarks, then WP-40 plots, WP-13 pushdown, and WP-37 plates once WP-08 lands. Sessions come first because comments, run records and the memory decision all key by session.
- **WP-14's writes go through the policy gate,** like agent writes: the gate classifies every write, a person's own edit is allowed without a prompt, and an agent's is a proposal (SW-23). One mechanism, not two.
- **WP-18's fixture:** a small Unison codebase created in the test from scratch (a handful of definitions, one rename, one dependency chain), not a downloaded one, so the test is hermetic.
- **WP-12:** the Supabase study's rejections become final through review by the coordinator, under the [[Scrutiny]] test. Daniel can always overturn an entry.
