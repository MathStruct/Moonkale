---
title: "North Star — the code graph"
description: The guiding post for Moonkale as an editor, from Daniel on 2026-10-14. A program is a graph in a database, not text in files, and that graph intersects a knowledge graph. What this asks of the editor, how today's pieces map onto it, and the decisions it constrains now.
tags: [architecture, vision, sophia, lenticulum, moc]
---
**From Daniel (2026-10-14):** the programming target is something like Unison, where the statements of the language are stored in a database. Imagine a future language in which types, functions and dataflows are all tracked, to a degree. Many e-graphs, annotations, intermediate representations and DSLs all live inside that graph. Think of a dependently typed Julia with Prolog-like elements, stored in a database like Unison's, checked by several systems (dependent, cubical, algebraic, …), and compiling down to assembly, LLVM and MLIR. This code graph intersects a knowledge graph. That is what Moonkale, as an editor, should be able to serve. `MathStruct/Sophia` is where these ideas are being worked out ([[Graph-Native Model]] has the content-addressed identity it needs).

This page is a direction, not a plan. Concrete work comes from it through the [[Work Packages]], ADRs and specs. Where a decision today would close this off, the decision changes or goes to [[Scrutiny]] with its reason.

## What the editor has to do

1. **Edit a graph whose text is a view.** A definition is a node, keyed by the hash of its normalised structure, with names as edges to it (Unison's model). Source text is one projection of it. Others are a tree, a dataflow diagram, a factor graph, or a proof state. Editing any projection edits the node. A diagram stays a diagram ([[Julia and Lenticulum]]); it is never round-tripped through text to be saved.
2. **Many representations of one thing, linked.** Surface syntax, the elaborated core term, e-graph classes, MLIR dialects, LLVM IR and assembly are layers of the same graph. Provenance edges run between them ("this instruction came from that expression, via that rewrite"). The editor shows one layer and lets you follow an edge down or up, the way the flow editor enters a subflow.
3. **E-graphs as first-class structure.** An e-class is a set of equivalent terms, and an e-node is an operator over e-classes. That is a hypergraph, not a tree. The engine's groups, hyperedges and simplices (spec [[031]] §2) are the drawing primitives. The editor needs ways to collapse an e-class to one representative, expand it, and see which rewrite produced which member.
4. **Several type systems over one term.** Dependent, cubical, algebraic (effects and handlers) and others each check, annotate or reject the same node. Their results are layered annotations ([[Annotations]]), shown and hidden like the engine's layers, never merged into one "type" field.
5. **Holes, goals and evidence.** A dependently typed program is written against goals (Lean, Agda). A hole is a node with an expected type, and filling it is an edit. Proofs and equivalence evidence ("these two IR fragments compute the same thing") are nodes with machine-checkable content, attached by edges to what they prove.
6. **Logic queries over the graph.** Prolog/Datalog-style rules ask "every caller of this effect", "every rewrite that fired on this term", "every paper that cites the theorem this function relies on". This is the query level above `Query::Text`. The table editor and the graph view show its results, as they do SQL and Cypher today.
7. **The code graph meets the knowledge graph.** Notes, papers, definitions in prose, datasets and the people who wrote them are nodes too. Wiki-links into code nodes and code nodes into concepts are ordinary edges ([[Graph-Native Model]]). The editor that holds both is the point: one navigation, one search, one history.
8. **Scale and change.** Millions of nodes, deeply shared subterms, a DAG more than a tree. Most of it is derived data, recomputed incrementally when a definition changes (the requirement found in [[External Code Intelligence Projects]]). History is a DAG of content hashes, not a list of file diffs.

## How today's pieces map onto it

| Need | Today | Gap |
|---|---|---|
| Content-addressed nodes, names as edges | `NodeId::from_content` in the model | no source uses it yet; Sophia is design-only |
| Graph as primary, text as a view | flow editor (diagram is the document), graph view | no structure editor over code; the code editor edits text files |
| Layers / IR levels | engine layers; flow subflows (enter/leave a level) | provenance edges between levels; a level switcher |
| E-graphs | engine groups, hyperedges, simplices (spec 031) | e-class collapse/expand; rewrite provenance |
| Type systems as annotations | [[Annotations]] (designed) | built nowhere |
| Factor graphs, dataflow, typed ports | flow editor: ports, roles, subflows, codegen (ADR-0020–0024) | live execution; Lenticulum's edge directions |
| Logic queries | `Query` IR + `Query::Text` (SQL, Cypher, kv, helix) | a Datalog/Prolog dialect; lifting rules |
| Knowledge graph | wiki-links, index, search, sources | cross-source links (R-16, Projects) |
| Scale | 100k nodes in the engine | coarse tier ([[Case Selector]]); persisted, incremental index |
| History as a hash DAG | entity log per folder | content-addressed versions ([[Version Management]]) |
| Components inside the graph | the flow editor's floating parameter editor | canvas components (planned, specs README) |

## Decisions this constrains now

- **Identity:** never assume a node is a file, a line or a text range. Keep `NodeId` opaque, and content addressing possible.
- **Graphs are DAGs and hypergraphs:** layouts, selection, history and the engine must not assume trees or simple graphs.
- **Representations are layers, not copies:** a new view of a thing links to it; it doesn't duplicate it.
- **Queries go through the source:** a new query language is a dialect on `Query::Text` plus lifting rules, not a special path.
- **Types and proofs are annotations with provenance:** who or what checked it, against which version.
- **Prefer the graph store's model over the file system's** whenever the two disagree. Files remain one source among several.
- **The proxy until Sophia exists:** track the Unison language's internals (definitions, types, dependency DAG) as a graph source. It is a real, content-addressed code database to design against.
