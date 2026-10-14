---
title: "North Star — the code graph"
description: The guiding post for Moonkale as an editor, from Daniel on 2026-10-14. Programs in many languages, parsed into one graph in a database, with proofs as annotations, intersecting a knowledge graph. What this asks of the editor, how today's pieces map onto it, and the decisions it constrains now.
tags: [architecture, vision, sophia, lenticulum, moc]
---
**From Daniel (2026-10-14):** the programming target is something like Unison, where the statements of the language are stored in a database. Imagine a future language in which types, functions and dataflows are all tracked, to a degree. Many e-graphs, annotations, intermediate representations and DSLs all live inside that graph. Think of a dependently typed Julia with Prolog-like elements, stored in a database like Unison's, checked by several systems (dependent, cubical, algebraic, …), and compiling down to assembly, LLVM and MLIR. This code graph intersects a knowledge graph. That is what Moonkale, as an editor, should be able to serve. `MathStruct/Sophia` is where these ideas are being worked out ([[Graph-Native Model]] has the content-addressed identity it needs).

This page is a direction, not a plan. Concrete work comes from it through the [[Work Packages]], ADRs and specs. Where a decision today would close this off, the decision changes or goes to [[Scrutiny]] with its reason.

**Amended the same day (Daniel):** this is planning for **many programming languages on the same graph**, not one future language. Existing languages (Rust, Julia, Python, C/C++, Lean, …) are **parsed into the graph**, so their definitions sit beside each other and beside the future language's. **Annotations can be proofs**: of equivalence, implication, memory safety, safe concurrency, and more.

## What the editor has to do

1. **Edit a graph whose text is a view.** A definition is a node, keyed by the hash of its normalised structure, with names as edges to it (Unison's model). Source text is one projection of it. Others are a tree, a dataflow diagram, a factor graph, or a proof state. Editing any projection edits the node. A diagram stays a diagram ([[Julia and Lenticulum]]); it is never round-tripped through text to be saved.
2. **Many languages, one graph.** Every language enters through a frontend that parses its source into the shared graph: definitions, types, calls, effects and dataflow, each tagged with its language and the frontend version that produced it. This is the index's job today, grown up: tree-sitter extractors for the core languages, and real elaborators (rust-analyzer, Julia's lowering, Lean's environment) where structure matters. Cross-language edges are ordinary edges: an FFI call, a binding, an equivalence. A file stays the import and export format for a language that lives in files. The graph is where its structure lives once parsed, and edits to the graph can be printed back.
3. **Many representations of one thing, linked.** Surface syntax, the elaborated core term, e-graph classes, MLIR dialects, LLVM IR and assembly are layers of the same graph. Provenance edges run between them ("this instruction came from that expression, via that rewrite"). The editor shows one layer and lets you follow an edge down or up, the way the flow editor enters a subflow.
4. **E-graphs as first-class structure.** An e-class is a set of equivalent terms, and an e-node is an operator over e-classes. That is a hypergraph, not a tree. The engine's groups, hyperedges and simplices (spec [[031]] §2) are the drawing primitives. The editor needs ways to collapse an e-class to one representative, expand it, and see which rewrite produced which member.
5. **Several type systems over one term.** Dependent, cubical, algebraic (effects and handlers) and others each check, annotate or reject the same node. Their results are layered annotations ([[Annotations]]), shown and hidden like the engine's layers, never merged into one "type" field.
6. **Holes, goals and evidence.** A dependently typed program is written against goals (Lean, Agda). A hole is a node with an expected type, and filling it is an edit.
7. **Proofs as annotations.** An annotation can carry a proof. It names what it proves and about which nodes: **equivalence** (two terms, possibly in different languages or IR levels, compute the same thing), **implication** (a property of one follows from another), **memory safety**, **safe concurrency** (data-race freedom, deadlock freedom), termination, bounds, refinement of a spec, and so on. Each proof has a checker (a type system, an SMT run, a Lean or Coq term, a model checker), the versions it was checked against, and a status. A proof whose subject changed is **stale**, shown as such, never silently kept. The editor shows which nodes are proven what, and lets you follow a proof to its evidence. A gap is a visible goal, not a missing badge.
8. **Logic queries over the graph.** Prolog/Datalog-style rules ask "every caller of this effect", "every rewrite that fired on this term", "every paper that cites the theorem this function relies on". This is the query level above `Query::Text`. The table editor and the graph view show its results, as they do SQL and Cypher today.
9. **The code graph meets the knowledge graph.** Notes, papers, definitions in prose, datasets and the people who wrote them are nodes too. Wiki-links into code nodes and code nodes into concepts are ordinary edges ([[Graph-Native Model]]). The editor that holds both is the point: one navigation, one search, one history.
10. **Scale and change.** Millions of nodes, deeply shared subterms, a DAG more than a tree. Most of it is derived data, recomputed incrementally when a definition changes (the requirement found in [[External Code Intelligence Projects]]). History is a DAG of content hashes, not a list of file diffs.
11. **Syntax is a user setting.** Because the stored program is a graph and text is a projection of it, how code *looks* can be each reader's choice, declared in their settings rather than fixed by the language. The proposal is in the next section.

## Proposal: declarative rendering settings (Daniel, 2026-10-14)

A user says **declaratively, in the settings**, how code is rendered. The stored graph never changes; only its projection does. Two people can read the same definition, one in Rust-like and one in Julia-like dress.

**What a setting can choose** (examples, not a closed list):

| Aspect | Choices |
|---|---|
| Keywords | a function as `fn` (Rust), `function` (Julia), `def` (Python); `let` / `local` / none |
| Indexing | 0-based (C++, Python, Rust) or 1-based (Julia, MATLAB, Fortran) |
| Blocks | braces, `end`, indentation |
| Type annotations | `x: T`, `x::T`, `T x`; shown, hidden, or inferred types shown inline |
| Macros and decorators | expanded, collapsed to a badge, or shown as written |
| Attributes and annotations | shown inline, in the gutter, or hidden (proofs, `#[derive]`, `@inline`, effects, lifetimes) |
| Implicit arguments | hidden (Lean's `{α}`) or shown |
| Operators | Unicode (`∘`, `≤`, `∀`) or ASCII (`.`, `<=`, `forall`) |
| Names | a name per locale or convention, since names are edges to a node, not the node |

**How it would be declared:** layered like every Moonkale setting. A user default, overridden per language, per project, and per view:

```toml
[render]                  # the reader's default dress
function   = "fn"
index_base = 0
blocks     = "braces"
operators  = "unicode"

[render.macros]
show = "collapsed"        # expanded | collapsed | as-written

[render.language.julia]   # code that came from Julia keeps its own habits
function   = "function"
index_base = 1
blocks     = "end"
```

**The rules that keep it honest:**
- **A projection is a view, never a rewrite.** Changing a setting changes no node, no hash and no history. Two readers with different settings edit the same graph.
- **It must be invertible where it is editable.** Text typed in a projection parses back through the same projection into the same graph. A setting that loses information makes the affected part display-only (shown, not editable, marked).
- **Semantic, not textual.** The index base is the hard example. Showing `a[0]` as `a[1]` is right only where the graph knows the expression *is* an index into a 0-based array; `x + 1` elsewhere stays as it is. A projection works on typed nodes, never on characters. Where the graph doesn't know, the canonical form shows, with a marker.
- **Shared artefacts use a stated projection.** A diff, a review comment, a copied snippet or a published page says which projection it was rendered in, and copying can choose: the reader's projection, the source language's own syntax, or canonical.
- **The language's own syntax is always one of the choices.** Printing back to a file (item 2) is the projection "as the language writes it".

A related case is a language model built as a **statistical logic program** (functions and relations, not a transformer). It is being developed, and its frontend requirements are LT-24 to LT-29 in [[Lenticulum Frontend Requirements]].

Prior art to study (requirements only, [[Scrutiny]] rules): Unison (names as display metadata over hashes), JetBrains MPS and Hazel (projectional editing), Lamdu, and the code editor's fold and live-preview decorations, which already replace and hide source ranges ([[Rust Code Editor Live Preview Plan]]).

## How today's pieces map onto it

| Need | Today | Gap |
|---|---|---|
| Content-addressed nodes, names as edges | `NodeId::from_content` in the model | no source uses it yet; Sophia is design-only |
| Graph as primary, text as a view | flow editor (diagram is the document), graph view | no structure editor over code; the code editor edits text files |
| Layers / IR levels | engine layers; flow subflows (enter/leave a level) | provenance edges between levels; a level switcher |
| E-graphs | engine groups, hyperedges, simplices (spec 031) | e-class collapse/expand; rewrite provenance |
| Many languages parsed into the graph | the index: tree-sitter symbols for Rust, Julia, Python ([[Core Languages]]) | calls/references, types, effects; real elaborators; printing back |
| Type systems as annotations | [[Annotations]] (designed) | built nowhere |
| Proofs as annotations (equivalence, implication, memory safety, safe concurrency) | — | the annotation's proof payload, checker and staleness; a proof status view |
| Factor graphs, dataflow, typed ports | flow editor: ports, roles, subflows, codegen (ADR-0020–0024) | live execution; Lenticulum's edge directions |
| Logic queries | `Query` IR + `Query::Text` (SQL, Cypher, kv, helix) | a Datalog/Prolog dialect; lifting rules |
| Knowledge graph | wiki-links, index, search, sources | cross-source links (R-16, Projects) |
| Scale | 100k nodes in the engine | coarse tier ([[Case Selector]]); persisted, incremental index |
| History as a hash DAG | entity log per folder | content-addressed versions ([[Version Management]]) |
| Rendering as a user setting | layered settings (user, workspace, per extension); the code editor's replace/hide decorations | projections over typed nodes; invertible parsing back |
| Components inside the graph | the flow editor's floating parameter editor | canvas components (planned, specs README) |

## Decisions this constrains now

- **Identity:** never assume a node is a file, a line or a text range. Keep `NodeId` opaque, and content addressing possible.
- **Graphs are DAGs and hypergraphs:** layouts, selection, history and the engine must not assume trees or simple graphs.
- **Representations are layers, not copies:** a new view of a thing links to it; it doesn't duplicate it.
- **Queries go through the source:** a new query language is a dialect on `Query::Text` plus lifting rules, not a special path.
- **Types and proofs are annotations with provenance:** who or what checked it, against which version. A proof goes stale when its subject changes.
- **Syntax belongs to the reader:** the stored graph must not encode one surface syntax (no keyword spelling, no index base baked into a literal), so a projection can choose them.
- **No language is privileged:** the graph's node and edge kinds must not assume one language's concepts. A language-specific kind is namespaced by its frontend.
- **Prefer the graph store's model over the file system's** whenever the two disagree. Files remain one source among several.
- **The proxy until Sophia exists:** track the Unison language's internals (definitions, types, dependency DAG) as a graph source. It is a real, content-addressed code database to design against.
