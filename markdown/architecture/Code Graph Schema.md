---
title: "Code Graph Schema — definitions, identity and edges for many languages on one graph"
description: "Draft spec (2026-10-15) for the North Star's code graph: what a definition is in each language, the two ids (where it is, and what it is: a content hash of its normalised structure), names as edges, the edge kinds code needs, layers from text to IR, and provenance on everything derived. Rust, Julia and Python are worked out as the first cases; a Unison codebase (WP-18) checks the model. Depends on ADR-0028 for whether the graph may be edited."
tags: [architecture, code-graph, spec, draft, north-star]
---
**Status:** draft, 2026-10-15 (Claude). It fills the gap noted in [[North Star]]: "a definition is a node, keyed by the hash of its normalised structure, with names as edges to it" had no schema behind it.
- **Identity, kinds, edges, layers and provenance** are Claude's to decide, under the North Star's constraints. They're reviewed like any spec.
- **Whether the code graph can be *edited*,** and where its truth lives, is [[ADR-0028 Files or the graph]], Daniel's decision. This page works under either answer and says where it differs.

It builds on:
- [[Graph-Native Model]]: nodes, edges, open kinds, properties;
- [[ADR-0015 The core model]]: `NodeId::from_content`;
- [[Indexing]]: tree-sitter symbols today;
- [[Annotations]] and [[Shared Work]] §5: tools as actors, provenance.

## 1. Principles (from the North Star's constraints)

1. **No language is privileged.** The shared kinds describe what every language has (a definition, a reference, a type). What only one language has is namespaced by its frontend (`rust:Impl`, `julia:Method`).
2. **A definition is the unit.** Not a file, not a line, not a text range. A file *contains* definitions, and the text range is a property of the occurrence, not of the definition.
3. **Two identities:**
   - **where a definition is**, a location id: stable across edits while it stays put;
   - **what it is**, a content id: stable across moves, renames and reformatting, and changed by any change in meaning.
4. **Names are edges, not identity.** Two names can point at one definition, and a rename changes an edge, not the node.
5. **Everything derived carries provenance:** which frontend, which version, and from which input hash. It goes stale with its input ([[ADR-0026 Memory and the store]]).
6. **Layers, not copies.** Syntax, definitions, elaborated core, IRs and machine code are layers linked by provenance edges. A view shows one layer and follows edges between them.
7. **Graphs, not trees.** Mutual recursion, generic instantiation, e-classes and shared subterms make it a DAG with cycles in places. Nothing may assume a tree.

## 2. Layers

| Layer | What | Stored? | Built by |
|---|---|---|---|
| **L0 Text** | the file (`File` node, `ContentRef::Text`) | the source (folder, git) | today |
| **L1 Syntax** | a lossless syntax tree: tree-sitter nodes plus trivia (comments and whitespace attached to tokens) | derived, on demand, cached by file hash; never persisted wholesale | the frontend. Required for printing edits back (ADR-0028 option B) |
| **L2 Definitions** | definitions, names, references, the shared edge kinds below | derived and **persisted** (the incremental index, WP-21) | the frontend's extractor |
| **L3 Elaborated** | what an elaborator knows: resolved types, trait and method dispatch, macro expansions, effects | derived, on demand | real elaborators (rust-analyzer, Julia's lowering and inference, Lean's environment) |
| **L4+ IR** | MIR or Julia's typed IR, MLIR dialects, LLVM IR, assembly | derived, on demand | compilers, as tools ([[Shared Work]] §5) |

Provenance edges (`LoweredTo` / `LoweredFrom`) connect a node in one layer to the nodes it became in the next. Today L0 exists, L2 exists in part (`Symbol` nodes), and the rest doesn't.

## 3. Definitions

A **definition** is a node of kind `Definition`, with:
- `def_kind`: one of the shared values below, or a namespaced one;
- `lang`: the language id;
- `frontend` and `frontend_version`;
- `content_id`: §4, once computed;
- `doc`: a reference to its doc comment, an annotation rather than part of the hash (§4).

**Shared `def_kind` values:**
- `function`: free functions, Julia methods, Python functions;
- `method`: one bound to a type;
- `type`: struct, enum, class, record, Julia `struct`, type alias;
- `interface`: trait, protocol, abstract type, Python ABC;
- `constant`: const, static, module-level binding;
- `module`: module, Julia `module`, Python module;
- `macro`;
- `test`;
- `field`: of a type;
- `variant`: of an enum or sum type.

### The first three languages

| Question | Rust | Julia | Python |
|---|---|---|---|
| The unit | each `fn`, `struct`, `enum`, `trait`, `const`, `static`, `type`, `macro_rules!`, `mod` | each **method** (one signature) is a `Definition{function}`; the generic function is `julia:GenericFunction`, with `MethodOf` edges from its methods | each `def`, `class`, module-level assignment to a name |
| Containers | `impl` blocks are **not** definitions: `rust:Impl` nodes group their items. An item in `impl Trait for T` gets `Implements(T → Trait)` on the impl and `MethodOf(item → T)` | `module` is a `Definition{module}`; `struct` fields are `field` definitions | `class` is a `Definition{type}`; its `def`s are `method` definitions with `MethodOf` |
| Code that isn't a definition | — | top-level statements other than definitions: a `julia:ModuleBody` node per module, so they're not lost | module-level statements other than definitions: one `python:ModuleBody` node per module (imports, side effects, `if __name__ == …`) |
| Generics | one definition per item; instantiations exist only in L3/L4 (`InstanceOf` edges) | one definition per method; specialisations exist only in L3/L4 | — |
| Conditional code | `#[cfg(...)]`: the definition exists once and carries `cfg` as a property, not one node per configuration | — | `if TYPE_CHECKING:` and similar: definitions keep a `condition` property |
| Macros | the call site is a reference to the macro definition; expansions are L3 | the same (`@macro`) | decorators: a `Decorates` edge from the decorator to the definition |

**An occurrence** (where a definition's text sits) is an edge, `Defines(File → Definition)`, with the byte range, line range and the L1 node path as properties. A definition with no file (store-backed, ADR-0028) has no `Defines` edge.

## 4. Identity

### The location id (where)

`NodeId::derive(source, "<relative path>#<symbol path>")`. The symbol path is the chain of containing definitions plus a disambiguator:
- an impl's self type and trait;
- a Julia method's signature;
- an index for duplicates.

This is today's derived id, extended past files. It stays stable while the definition stays put, even if its body changes. Editors, selections, comments and links use it.

### The content id (what)

`NodeId::from_content(BLAKE3(canonical bytes))`. The canonical bytes are a serialisation of the definition's **normalised** structure:
1. **Tagged:** the language id and the normaliser's version are part of the input. A normaliser change gives new ids rather than silently equal ones.
2. **Trivia dropped:** comments, whitespace, formatting and doc comments. Docs are annotations on the definition, so editing a doc doesn't change what the code is.
3. **Locals renamed:** parameters and local bindings get de Bruijn-style positions, so `fn f(x) { x }` and `fn f(y) { y }` hash the same.
4. **The definition's own name excluded:** names are edges (§5), so a rename doesn't change the content id.
5. **References to other definitions** are replaced by the referenced definition's content id where it's resolved (Unison's rule: a change to a dependency gives a dependant a new id). Where it isn't resolved, a stable placeholder is used: the reference's name path plus `unresolved`, so it is never the empty string.
6. **Mutual recursion:** a strongly connected group of definitions is hashed together as one component. Each member's id is the component hash plus its index, ordered canonically, as Unison does.
7. **Language-meaningful spelling kept:** a Rust lifetime name, a Python keyword argument's name, and anything the language observes. The frontend decides, per language, what is observable. That list is the normaliser's specification and is versioned with it.

**When it's computed:** lazily, for the definitions a feature needs (history across moves, equivalence proofs, "is this the same function"). It's cached by the L1 input hash, and isn't eagerly computed for a million nodes.

**What it's used for:**
- **file-backed code** (ADR-0028 A or B): a derived identity that finds a definition again after moves and renames;
- **store-backed code** (Sophia, Unison): *the* identity.

## 5. Names

A **name** is a `Name` node: a namespace path, for example `crate::graph::layout` plus `relax` (Rust), `Lenticulum.Nodes.factor` (Julia), or `pkg.mod.fn` (Python). It has a `Names(Name → Definition)` edge to the *location* id. Store-backed sources point names at the content id.

- A rename moves or recreates the `Name`.
- Re-exports (`pub use`, Julia `export`, Python `__all__` or `from x import y`) are further `Names` edges into the same definition, with a `visibility` property.
- Two names for one definition is normal.

## 6. Edge kinds

**Shared kinds, which every frontend uses where its language has the concept:**

| Kind | From → to | Notes |
|---|---|---|
| `Contains` | module, type or impl → member | the lexical structure |
| `Defines` | `File` → `Definition` | the occurrence, with ranges (§3) |
| `Names` | `Name` → `Definition` | §5 |
| `References` | definition → definition | any use; the base kind the others refine |
| `Calls` | definition → definition | a call site; the count and the ranges are properties (WP-22) |
| `TypeOf` | definition → type definition | a declared type, or an inferred one (L3, with provenance) |
| `Implements` | type or impl → interface | trait impl, subtype of an abstract type, ABC registration |
| `MethodOf` | method → type, or method → generic function | |
| `Imports` | module → module or definition | `use`, `using`/`import`, `import`/`from` |
| `Decorates` | decorator → definition | Python decorators, Rust attribute macros, Julia macros on a definition |
| `Overrides` | method → method | an override of a base or default method |
| `InstanceOf` | L3 instance → generic definition | monomorphisation, specialisation |
| `LoweredTo` | node in layer n → node in layer n+1 | provenance between layers (§2) |
| `Tests` | test → definition | from naming conventions, or the test calling it directly |
| `Unresolved` | definition → an `Unresolved` node holding the name path | references the frontend couldn't resolve, kept so they're visible and can be resolved later |

**Namespaced kinds,** for a concept only one language has: `rust:Lifetime`, `julia:Specializes`, `python:Monkeypatches`. Views treat an unknown kind as `References` for layout and filters.

**Equivalence and other proofs** aren't edges of the code graph. They're annotations with a proof payload ([[Annotations]], WP-19) that name their subjects by content id, so a proof survives moves and goes stale when the content changes.

## 7. Provenance

Every node and edge a frontend or tool produces carries:
- `by`: the frontend or tool id;
- `by_version`;
- `input`: the hash of the input it was derived from (the L1 file hash, or a definition's content id);
- `derived_at`.

A node is **stale** when its input hash no longer matches. Views show stale results as stale ([[Shared Work]] §5, WP-54) and never mix them silently with fresh ones.

## 8. Scale

- **L2 for a large workspace:** about 10⁵ to 10⁶ definitions and a few times as many edges. It's persisted incrementally (WP-21): a change to one file re-derives that file's definitions and the edges into them, and nothing else.
- **L1, L3 and L4** are computed for what's on screen or asked for, and dropped by the cache budgets of ADR-0026.
- **Content ids** are computed lazily (§4).

## 9. How it's checked

- **Fixtures** in Rust, Julia and Python, each with these tests:
  - a renamed local keeps the content id;
  - a function moved to another file keeps its content id and gets a new location id;
  - a comment or doc edit changes neither id;
  - a body change keeps the location id and changes the content id, and the content ids of its dependants;
  - a mutually recursive pair gets a shared component;
  - a renamed function keeps both ids, and only the `Name` changes;
  - an unresolved reference shows up as `Unresolved`.
- **The Unison source (WP-18):** a Unison codebase already *is* this model (definitions by hash, names as metadata, a dependency DAG). Loading it into §3 to §6 without new shared kinds is the test that the schema is general. If it needs new shared kinds, the schema changes before the frontends multiply.

## 10. Order

1. The shared kinds and the `Definition` node in `moonkale-core` as constants: kinds are open strings, so this is a naming change, not a model change. Today's `Symbol` nodes are migrated to `Definition` with `def_kind`.
2. Location ids for definitions in the index (Rust, Julia, Python), with `Contains`, `Defines` and `Names`.
3. `Calls`, `References` and `Imports` (WP-22), and `Unresolved`.
4. The Unison source against this schema (WP-18).
5. Content ids, behind a feature, with the fixture tests of §9.
6. L1 lossless syntax and printing back: only if ADR-0028 chooses B (or C).

## 11. Open questions (non-fundamental: decided when their step comes)

- The exact normalisation per language (what's observable): written per frontend at step 5, versioned with the normaliser.
- Rust macros that generate items (`macro_rules!` making functions): are generated items definitions with a `GeneratedBy` edge, or only L3? Leaning towards L3, with the call site as a reference.
- Python's dynamic definitions (`setattr`, metaclasses): out of scope for L2. An elaborator can add them at L3.
- How a `Definition` of a store-backed language (Sophia) differs: it doesn't, apart from having no `Defines` edge. To be confirmed with WP-18.
