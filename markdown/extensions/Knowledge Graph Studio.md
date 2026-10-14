---
title: "Knowledge Graph Studio — RDF stores and layered memory in Moonkale"
description: Requirements for a Moonkale extension that is the frontend for two embedded graph databases on SQLite, oxilite (RDF, SPARQL, reasoning, SHACL) and tiramemsu (layered, bitemporal, never-forget memory), using their Rust crates as they are. Adapted from oxilite studio's published spec and the two projects' documentation. Proposed 2026-10-14.
tags: [extension, rdf, sparql, knowledge-graph, spec, proposal]
---
**Status:** proposal (2026-10-14), not scheduled. Work packages WP-31 to WP-35 in [[Work Packages]].

**Sources.** The requirements are taken from public documents only, under the [[Scrutiny]] rules (requirements and behaviour, no code):
- [oxilite studio](https://github.com/Volland/oxilite-studio): its OpenSpec `studio-extension` spec and its `lat.md` architecture, decisions and milestones (Apache-2.0);
- [oxilite](https://github.com/Volland/oxilite) (Apache-2.0);
- [tiramemsu](https://github.com/Volland/tiramemsu) (Apache-2.0).

What Moonkale rejects or changes is at the end and in [[Scrutiny]].

## What it is

Two embedded graph databases, each one SQLite file:
- **oxilite:** an Oxigraph-compatible RDF store and SPARQL 1.1 engine compiled to plain SQL, which also runs on Turso and Cloudflare D1. It adds openCypher, Datalog, RDFS/OWL reasoning with provenance, SHACL/ShEx validation, a schema registry, versioning and vectors.
- **tiramemsu:** layered, bitemporal, never-forget memory for agents. Every fact `(eid, s, p, o)` has an id, so provenance, confidence and beliefs are statements about statements. It has transaction and valid time, `supersede` and `confirm`, conflict review, saved answers that go stale, SPARQL 1.2 annotations and Cypher over one store.

**The extension is their frontend in Moonkale. It does not reimplement either backend.** It links the published crates (`oxilite` 0.9, `tiramemsu` 0.7) and drives them through their own APIs, and for language features it uses oxilite's own language server (`oxilite studio-server`). It lives in its own repository, like `moonkale-julia`, depends on an `ext-api` tag, and is opt-in in the distribution.

Its users, as oxilite studio defines them:
- **modellers:** ontologies, shapes, rules; reasoning and validation;
- **app developers:** querying a real store, tuning plans;
- **agents:** through tools.

## Requirements

`KG-nn` are this extension's requirements. "Moonkale" names the existing piece each one builds on.

### Connections and stores

- **KG-01 Two kinds of store.** A **project store** is derived from the RDF files of an open folder (one named graph per file, or as a manifest says), lives in a git-ignored scratch file, and is rebuilt on reload. Its edits are reported as temporary. An **attached store** is a real database (a local `.sqlite`, a local D1 file, a remote D1 over HTTP), and its edits persist. *Moonkale:* both are **Sources** in the Sources panel. The project store is derived like the index; an attached store is a database source like SQLite or DuckDB today.
- **KG-02 Read-only by default, writes confirmed.** Remote stores open read-only, and a write to an attached store asks first (with D1, it also shows the estimated billed rows). Each D1 result shows its requests and the rows read and written. *Moonkale:* the source's `read_only` and the policy gate that already guards agent writes; credentials in Moonkale's secret store.
- **KG-03 Documents pin their connection.** A query file whose first comment says `oxilite: connection = <target>` (in its language's comment syntax) runs, explains and completes on that store without changing the active one. A missing file is never created by resolving a pin. Unpinned documents use the active connection, which the status bar shows. *Moonkale:* a per-document source binding, also usable by SQL and Cypher documents generally.
- **KG-04 tiramemsu files are stores too:** opened as sources with their views (see KG-27 onward).

### Languages

- **KG-05 Language support:** Turtle/TriG, SPARQL (`.rq`, `.ru`), Datalog (`.dl`), openCypher (`.cypher`) and ShEx get diagnostics, completion from the store's own vocabulary (predicates by use with counts, classes after `a`, missing `PREFIX` lines added), hover, go-to-definition and references across files. Completion follows the document's pinned connection. *Moonkale:* the code editor's LSP client, with `oxilite studio-server` as the server. Discovery is hard-coded per language today, so it needs a **language-server contribution point** for extensions. That is a general gain.
- **KG-06 Run and explain:** Ctrl+Enter runs the document (a query, a rule program, a Cypher statement), and Ctrl+Shift+E shows the SQL it compiles to with planner notes; a structured plan tree comes later. Long operations report progress and can be cancelled.
- **KG-07 Query history:** every run is kept per workspace with its connection and a result summary, and reopens as a query. *Moonkale:* history entries are nodes, so they are linkable from notes.

### Results

- **KG-08 Table:** every result becomes rows: solutions as they are, graphs as subject–predicate–object, booleans as one cell. Terms are typed and coloured by kind (IRI, literal with datatype or language, blank node, triple term). IRIs open the resource view. *Moonkale:* the virtualised table grid (no row cap); server-side paging later.
- **KG-09 Graph:** CONSTRUCT and DESCRIBE results, Cypher nodes, relationships and paths, and a resource's neighbourhood. Inferred edges are dashed, and a table/graph toggle sits on every result that holds a graph. Small graphs are layered along edge direction, larger ones force-laid out, and a relationship repeated across paths is drawn once. *Moonkale:* the graph engine. **No 1 500-node cap**; the engine draws 100 k.
- **KG-10 Results name their connection:** every result and notebook output says which store produced it.

### Exploring a store

- **KG-11 Store explorer:** graphs (with their registry role and targets), the class hierarchy with asserted and inferred instance counts, properties by use, files and their roles, and prefixes. *Moonkale:* the Sources tree.
- **KG-12 Resource view:** a resource's statements in both directions, with inferred ones badged by producer, plus a link to the graph neighbourhood. *Moonkale:* a node page, reachable from any IRI anywhere, wiki-links in notes included.
- **KG-13 Ontology diagram:** classes with datatype properties and instance counts, subclass links and object properties, in the graph view.
- **KG-14 Full-text search** over string literals into a results panel. *Moonkale:* the search panel, with the store as one more corpus.

### Reasoning, validation, proof

- **KG-15 Reasoning per connection or query:** none, RDFS, OWL QL rewriting, or materialised OWL 2 RL plus Datalog rules, each materialised under its producer's name.
- **KG-16 "Why?"** Every inferred statement opens its **proof tree**: each inference with its producer and rule, down to asserted statements that open at their source line. *Moonkale:* a proof is an annotation ([[North Star]] §7); this is its first real user.
- **KG-17 Live SHACL (and ShEx):** validation runs on save over asserted plus inferred data. Each violation is a diagnostic at the focus triple's source line, linked to its shape, and a report view groups results by shape or focus node and exports an `sh:ValidationReport`. A toggle restricts it to asserted data.
- **KG-18 Rules debugger:** for a `.dl` file, each rule's body matches and head facts, empty and exploding rules flagged, and the strata.

### Schema registry

- **KG-19 Registry view:** a store's registered graphs with role, active flag, targets, imports, version and size; the registry's problems; whether the system graphs are installed at the current vocabulary version. A server too old for it says so; it never shows an empty view.
- **KG-20 Mapping graph:** every named graph is a node styled by role. Edges run from each schema graph to the graphs it applies to, and from each ontology to the ontologies it imports (dashed). A schema applying to every graph links to an "All graphs" hub, which expands into one edge per data graph. Inactive registrations are faded, and empty targets are drawn as missing. *Moonkale:* engine styles (stroke, dash, opacity, groups) already cover it.
- **KG-21 Effective schemas per graph:** the active schemas that apply to each data graph, and why (a direct target, all graphs, or an import chain).
- **KG-22 Registry edits:** register, add a role, retarget, activate, deactivate, unregister, drop (naming the triples it deletes), install system graphs. Attached stores confirm, read-only ones refuse, and project-store edits are flagged as lost on reload. The view refreshes after each edit. The registry is read and written only through oxilite's own registry API, never through hand-written SPARQL.

### Projects, tests, notebooks, agents

- **KG-23 Convention first, manifest optional:** any folder of RDF files works. An `oxilite.toml` manifest, once present, defines graphs, roles, reasoning, rules and tests. "New project" scaffolds a folder that passes `oxilite check`, and never overwrites a file.
- **KG-24 Knowledge-graph tests:** the manifest's tests (query results, SHACL conformance or expected violations, entailments) run from the editor with diffs, and "update snapshot" rewrites an expected file. CI runs them with `oxilite check`.
- **KG-25 Notebooks:** Markdown with SPARQL, Datalog and Cypher cells. Each cell runs on the notebook's chosen connection, which is remembered in the file and re-attached on open. Outputs are saved with the connection that produced them. *Moonkale:* the [[Notebook Editor]] (designed only); this is a reason to build it.
- **KG-26 Agents:** oxilite's MCP tools (query, schema, validate, why) available to Moonkale's agent panel and to external agents, and tiramemsu's MCP memory tools likewise. *Moonkale:* the agent panel and the MCP bridge exist, so no separate chat is needed.

### Layered memory (tiramemsu)

tiramemsu has no frontend of its own yet. These requirements come from what its documented features need to be seen and used.

- **KG-27 Statements as nodes:** a fact with layers is drawn as a junction on its edge (the engine's hyperedge junction). Its layers (confidence, source, belief, supported-by) hang off the junction and are shown or hidden as engine layers. Cypher's view of a relationship as a `:Statement` node and SPARQL's `{| … |}` annotations are two notations for one drawing.
- **KG-28 Two clocks:** a time control over transaction time ("what the store believed then", `as_of`) and valid time ("what was true then", `valid_at`). It applies to the table, the graph and the resource view at once, and every view says which time it shows. History mode shows every version.
- **KG-29 A fact's life:** the supersede chain of a fact, its confirmations, its retraction with reason and author, and the layers replayed onto each version.
- **KG-30 Before you retract:** `dependents(eid)` shown as a preview of what a retraction would take with it. No delete exists in the UI; retracting is the only removal.
- **KG-31 Conflict review:** subject–predicate pairs with distinct values at overlapping valid time, each with its evidence (statement ids, authors, sources, confirmations, confidence when stated). No made-up score, and no write from the view.
- **KG-32 Saved answers:** saved queries with their view and result, marked *stale* when a cited statement changes and *recheck* when another write could matter. The marks are visible, and only a re-run clears them.
- **KG-33 Bundles:** export a fact with its layers and evidence; import shows a dry run first (proposed and reused facts, cardinality retractions, schema failures).
- **KG-34 Named graphs as containers:** a graph named by a node or a statement draws as a group around its members (engine groups); folding and unfolding a metagraph is a view action, not an edit.
- **KG-35 Paths:** reachability and shortest paths, time-respecting ones included, drawn on the graph with each hop's earliest arrival. Speculative `with` / `dry_run` changes are shown as a preview that leaves no trace.
- **KG-36 Budgets:** every query from the UI carries a deadline and row limits, can be cancelled, and is never silently truncated.

## Why a Moonkale extension, not an editor plugin

These follow from the requirements, not from preference:
- **The engines link in.** Both backends are Rust, and so is Moonkale, so the extension calls the crates in-process on desktop and on the server. An Obsidian or VS Code plugin needs a JS binding, a wasm build or a sidecar process for the same.
- **One graph for notes, code and data.** A note can wiki-link an IRI or a statement, and a Cypher result sits in the same graph view as the code index and a LadybugDB source.
- **Scale:** the graph engine draws 100 k nodes, so no cap.
- **Proofs, layers and time are first-class drawing concepts** in the engine (junctions, layers, groups, styles), not decorations bolted on.
- **The same extension runs on desktop, web and Android** (the phone through the server).
- **Agents are already there:** sessions, the policy gate for writes, MCP.

## Taken, adapted, rejected

- **Taken as they are:**
  - the two kinds of store;
  - pinned documents;
  - completion from the store's vocabulary;
  - "why?" proof trees;
  - live SHACL on asserted plus inferred data;
  - the registry view and its rules;
  - the manifest and its tests;
  - notebooks that remember their connection;
  - agents through MCP tools.
- **Adapted:**
  - React webviews with Cytoscape become Dioxus and the graph engine, with the table/graph toggle kept;
  - VS Code SecretStorage becomes Moonkale's secret store;
  - the Problems panel and Test Explorer become Moonkale's diagnostics and a test view;
  - VS Code's MCP provider becomes the agent panel's MCP bridge.
- **Rejected or limited** ([[Scrutiny]]):
  - the 1 500-node graph cap;
  - results capped by a row limit instead of virtualised or paged;
  - workarounds that exist only for VS Code webviews (the ready-mailbox, link-styled buttons).

## Steps (when scheduled)

1. **Open an oxilite store as a source** (WP-31): SPARQL in the table panel, CONSTRUCT and DESCRIBE in the graph view, read-only.
2. **Language features** (WP-32): a language-server contribution point, then `oxilite studio-server` for SPARQL, Turtle, Datalog and Cypher; run and explain.
3. **Trust** (WP-33): the resource view, inferred badges, "why?" proof trees, live SHACL diagnostics.
4. **The registry** (WP-34): the mapping graph, effective schemas, edits through the registry API.
5. **Layered memory** (WP-35): tiramemsu as a source, statement junctions, the two-clock control, conflict review, a fact's life, saved answers.
