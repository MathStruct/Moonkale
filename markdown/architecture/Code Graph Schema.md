---
title: "Code Graph Schema — definitions, identity and edges for many languages on one graph"
description: "Draft spec (2026-10-15) for the North Star's code graph: what a definition is in each language, the two ids (where it is, and what it is: a content hash of its normalised structure), names as edges, the edge kinds code needs, layers from text to IR, and provenance on everything derived. Rust, Julia and Python are worked out as the first cases; a Unison codebase (WP-18) checks the model. ADR-0028 (accepted: B) lets graph views edit file-backed code through text patches."
tags: [architecture, code-graph, spec, draft, north-star]
---
**Status:** draft, 2026-10-15 (Claude). It fills the gap noted in [[North Star]]: "a definition is a node, keyed by the hash of its normalised structure, with names as edges to it" had no schema behind it.
- **Identity, kinds, edges, layers and provenance** are Claude's to decide, under the North Star's constraints. They're reviewed like any spec.
- **Where the truth lives** is [[ADR-0028 Files or the graph]], accepted 2026-10-15 as option B: files stay the truth for file-backed code, graph-side edits print back as text patches (Rust first), and store-backed sources hold their own truth.

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

Provenance edges (`LoweredTo` / `LoweredFrom`) connect a node in one layer to the nodes it became in the next. Today L0 exists, the index emits L2 `Definition`/`Name` nodes and syntactic references for Rust, Julia and Python (WP-65/WP-22), and a default-off Rust L1 snapshot/patch planner with revision-aware document integration and graph previews exists (WP-67 slices 1–5); the other derived layers remain unbuilt.

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

### Sophia's ids (2026-10-20)

Sophia's specs (in its own repository, `vault/Specifications/`, drafts of 2026-10-20) fill in the store-backed case. Its store is language-agnostic (SPEC-001 Sophia Store v0 there), and the Moonkale side is SPEC-004 Moonkale Interface.
- **A definition's id is `from_content(digest)`**, with Sophia's digest. Each definition row carries `lang` and its normaliser (`frontend`, `frontend_version`), which is rule 1 above. So a Rust or Julia definition normalised by WP-66 can live in the same store.
- **Every definition is a component member.** A non-recursive definition is a component of size 1, so rule 6 has no special case.
- **Nominal entities** are the one exception to content addressing: generic functions (with their method tables) and nominal types get a *fresh* id when they are created, and Moonkale shows them as `from_content(nominal id)`. What changes over time are their versioned relations (methods added or removed), not the id. That's how a Julia generic function keeps one identity while its methods change.
- **Lineage** (`sophia:Lineage`, `derive(source, "lineage:" + first digest)`) links a definition to the definitions it replaced. Moonkale's own review comments on Sophia code anchor to the lineage id, so they survive edits and renames.

## 5. Names

A **name** is a `Name` node: a namespace path, for example `crate::graph::layout` plus `relax` (Rust), `Lenticulum.Nodes.factor` (Julia), or `pkg.mod.fn` (Python). It has a `Names(Name → Definition)` edge to the *location* id. Store-backed sources point names at the content id.

- A rename moves or recreates the `Name`. For file-backed code the location id also changes because the symbol path includes the name; only the future content id is rename-invariant.
- Re-exports (`pub use`, Julia `export`, Python `__all__` or `from x import y`) are further `Names` edges into the same definition, with a `visibility` property.
- Two names for one definition is normal.
- **For store-backed sources the name is primary** (SPEC-004 §2–§3). It's what you open, edit and navigate: `WriteText` on a name parses the text, stores a new definition and moves the name, while a definition itself is immutable and opens read-only. The graph panel hides `Name` nodes for file-backed code, where they're metadata, and has to show them for a Sophia source.
- **Path spelling:** Moonkale paths use `/`, Sophia names use `.`; the adapter maps `a/b/c` ↔ `a.b.c` and rejects segments with `/`, `.`, whitespace or control characters.

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
  - a renamed function keeps its content id, changes its location id, and changes the `Name`;
  - an unresolved reference shows up as `Unresolved`.
- **The Unison source (WP-18):** a Unison codebase already *is* this model (definitions by hash, names as metadata, a dependency DAG). Loading it into §3 to §6 without new shared kinds is the test that the schema is general. If it needs new shared kinds, the schema changes before the frontends multiply.

### WP-18 result (2026-10-17)

The default-off Unison source implements the store-backed form of §3–§6 with existing `Definition`, `Name`, `Directory`, `Contains`, `Names`, `References` and `TypeOf`: **no new shared kinds**. A hermetic UCM 1.5 transcript checks definition-id stability through rename, two aliases to one definition, dependency hash propagation after releasing an old alias, and shared component/different member ids for mutual recursion. Types, abilities, constructors and tests map through `def_kind`. Hash references include constructor selectors; intrinsic builtins retain UCM's `##name` identity instead of pretending to have content digests. Namespaces are Directory nodes; the source has no file occurrences or `Defines` edges. `TypeOf` only targets an exact known type reference, not every constituent in a function signature.

This is a finite, read-only snapshot, not a live codebase bridge: it imports projects/branches through an owned loopback UCM HTTP server and stops that process before returning. Hash-only display requests avoid UCM's named documentation-evaluation path. The source opener and graph panel accept store-backed code; a native test checks the fixture's actual renderer payload. Limits, canonical reference-byte encoding for `from_content`, API references and exclusions are in `packages/sources/unison/sources-unison.md`. Full type-expression nodes, store edits, history/watch and a dialect remain later work. §7 provenance is not implemented beyond frontend metadata and content/component references in this slice.

## 10. Order

1. The shared kinds and the `Definition` node in `moonkale-core` as constants: kinds are open strings, so this is a naming change, not a model change. Today's `Symbol` nodes are migrated to `Definition` with `def_kind`.
2. Location ids for definitions in the index (Rust, Julia, Python), with `Contains`, `Defines` and `Names`.
3. `Calls`, `References` and `Imports` (WP-22), and `Unresolved`.
4. The Unison source against this schema (WP-18).
5. Content ids, behind a feature, with the fixture tests of §9.
6. L1 lossless syntax and printing back (WP-67; ADR-0028 accepted B), Rust first.

## 11. Open questions (non-fundamental: decided when their step comes)

- The exact normalisation per language (what's observable): written per frontend at step 5, versioned with the normaliser.
- Rust macros that generate items (`macro_rules!` making functions): are generated items definitions with a `GeneratedBy` edge, or only L3? Leaning towards L3, with the call site as a reference.
- Python's dynamic definitions (`setattr`, metaclasses): out of scope for L2. An elaborator can add them at L3.
- ~~How a `Definition` of a store-backed language (Sophia) differs~~ **Answered** by WP-18 (2026-10-17) and Sophia's SPEC-004 (2026-10-20): it doesn't, apart from having no `Defines` edge. The shared kinds are enough: `Directory`, `Name`, `Definition`, `Annotation`, plus `Contains`, `Names`, `References`, `Calls`, `Tests`, `TypeOf`, `MethodOf` and `Annotates`. Sophia-only concepts are namespaced: `sophia:Term`, `sophia:Type` (an inferred compound type that is not a stored definition), `sophia:Lineage` and `sophia:Settings` nodes, and `sophia:Subterm`, `sophia:Supersedes` and `sophia:InLineage` edges. `sophia:Subterm` isn't `Contains`, because shared subterms make a DAG, while views treat `Contains` as a tree.
- Still open for Sophia, decided in WP-68 or later:
  - **annotation edits:** a new `Annotate`/`Unannotate` op, or a pseudo-text node per annotation;
  - **`datalog`:** a new `TextDialect` (v0 has read-only `sql` only);
  - **a `Code` family:** a `moonkale-core` change, out of v0, where a Sophia source is family `Graph`.
- **Rendering profiles** (Sophia's SPEC-005): how code is printed (`fn`/`def`/`function`, 0- or 1-based indexing, …) is stored per reader in the Sophia database, not in Moonkale settings. Julia-like is the default. Moonkale sees the profile as one `sophia:Settings` node edited as text. A profile switch reprints every definition and changes no id. Whether Moonkale's own settings for file-backed code move there too is a later question (SR-74, Sophia as Moonkale's parsing and storage engine).

## Implementation contract (WP-65/WP-22, 2026-10-15)

- The existing core represents kinds with built-in enum variants plus `Custom(String)`, rather than a bare string. New vocabulary is additive string constants in `moonkale_core::graph::code`, used as `Custom` values; existing `Contains`, `Defines`, `Calls` and `References` keep their wire encoding. Legacy `Symbol` values remain readable, and the index accepts a legacy `Symbol` filter for definitions. No node/edge model or enum-variant change is needed.
- This step migrates the existing extractors' items. Rust impl containers become `rust:Impl`, not definitions; they retain ranged `Defines` occurrences so the existing graph navigation can open their file. Bound Rust/Python functions carry `def_kind=method`. Extraction of additional fields, variants, module-level assignments, Julia generic-function containers and non-definition module-body nodes remains frontend coverage work; this step does not claim those units are implemented.
- Location keys keep the existing `path#kind:name/…` form. Rust impl keys include self type and optional trait. Julia method keys use the whitespace-normalised complete signature (including `where` and return annotations), not their order among other overloads. Identical keys receive `~2`, `~3`, etc. A duplicate inserted before another identical signature may change its duplicate index; semantic equivalence and move tracking belong to WP-66.
- `Defines` carries half-open UTF-8 `start_byte`/`end_byte`, 1-based `start_line`/`end_line`, and `syntax_path` (slash-separated child indices, including unnamed tokens, in the parsed tree). `line` remains on nodes for legacy consumers. `Names` points from a file-qualified namespace name to each definition; Julia overloads share a name node. Name nodes are metadata and excluded from the graph panel's normal symbol filter.
- Resolution is deliberately syntactic and same-file: only an unambiguous visible bare name resolves. Possible local shadows, overload dispatch, qualified/dynamic calls and external imports remain `Unresolved` nodes with the spelling in the label. A `Calls` edge also produces `References`. Imports retain an `Imports` edge to their unresolved target. An `Unresolved` edge additionally marks each unresolved use. No compiler, dependency loader, macro expansion, code execution, or network lookup runs.
- A file represents its implicit module until explicit module/body coverage is added. Top-level uses and imports originate there; uses within explicit definitions originate in the innermost extracted item. This extends the edge table's definition/module origins to `File` for implicit modules. Rust impl occurrences are likewise an explicit extension of the occurrence table.
- Calls/references aggregate by `(from,to,kind)` with `count` and a `ranges` text property: comma-separated `start:end` UTF-8 ranges. `start_byte`/`end_byte` retain the first encountered occurrence. Ranges are not promised in source order. The property representation follows today's scalar `Value` model; a future typed range-list value may replace it with a migration.
- Provenance uses `by=moonkale-index`, the crate version as `by_version`, an `input` UUID-v5 digest of the exact file bytes under the `index-input-v1` domain, and Unix seconds in `derived_at`. This input digest is independent of mtime-based source versions and is not a WP-66 content id. It is for freshness, not cryptographic authentication. Wiki-link derivatives carry the same provenance. Mirrored folder nodes and folder containment keep their source metadata.
- The existing 5,000-file / 512 KiB-per-text-file walk bounds apply; the reference pass uses an explicit traversal stack. Refresh atomically replaces a file's definitions, names, unresolved nodes and derived edges. There is no cross-file semantic resolution or persistence in this step (WP-21 remains separate).

The rename test above was corrected because location paths include names (§4); keeping both ids through a rename would contradict that definition. The implicit-module and impl-occurrence exceptions make current navigation possible without pretending L1 printing or full frontend coverage exists. Content ids (WP-66) and printing back (WP-67) are unchanged and unimplemented here.

## WP-66 normalizer (v1 2026-10-16, v5 2026-10-20)

**Implemented, experimental and off by default:** `moonkale-index/content-ids`. The normal form and the hashing are their own crate, `moonkale-content-id` (`packages/services/content-id`, 2026-10-20), shared with Sophia so a definition can have one id in both (SR-116). The index adds reference resolution and the cache. The crate's `golden/` fixtures carry Moonkale's ids and reference spans for a cross-check. **v2** fixes resolution: Julia and Python parameter types and defaults (`p::Point`) are uses, not local names. In v1, a method typed with a same-file struct didn't depend on it. **v3**: a Julia `struct` no longer references its own name, and a Rust struct expression's type is a use; Sophia's cross-check found both. **v4** adds Julia's local scopes (loops, closures, `do`, `let`, `try`, comprehensions), varargs, keyword parameters, destructuring and anonymous `::T` parameters; real-code coverage on LinearAlgebra rose from about 69% to 99% of definitions. **v5** resolves Julia references per **module**, as Julia does. A package is one module over its `include`d files, found from `Project.toml`. A name resolves only to a unique, unimported binding in its module, and calls to a function with several methods stay unresolved (they are edges to the generic function, not part of ids). Rules agreed with Sophia in its SPEC-008. Rust and Python keep same-file resolution. See `content-id.md` for the version list and rules. These are conservative **syntactic** identities under the rules below, not a proof of language-level equivalence. Runtime introspection, external modules, compiler flags, macro expansion and dynamic dispatch are not modeled. Files and location ids remain authoritative. No application enables the feature automatically and no UI consumer or remote query dialect is added.

### Request and encoding

`IndexSource::content_id(location_id)` explicitly requests a definition and its reachable same-file definitions. The async API returns `ContentIdentity { id, digest, normalizer_version, component_digest, component_index, component_size }`; the UUID is `NodeId::from_content` of the full BLAKE3 member digest. The existing definition node receives `content_id` and the corresponding `content_digest`, `content_normalizer`, `content_component`, `content_component_index` and `content_component_size` properties only after the complete request succeeds. Ordinary build/search/graph queries do not compute hashes. No core kind, id encoding or source protocol changes.

Canonical fields have a little-endian u32 tag length, tag bytes, little-endian u32 payload length and payload bytes. The syntax stream includes language, normalizer version, node kinds, ordered children and leaf tokens, with explicit open/close fields. It has no path, source id, timestamp or byte position. The declaration's own name is a dedicated empty token. Other observable names remain. Lexical value bindings use declaration slots and `(scope depth, slot)` references; Rust block bindings activate after the initializer and shadowing allocates a fresh slot. Python function locals are collected before body traversal. Julia positional parameters and function-local assignments use the same scheme.

References resolved by the existing conservative same-file extractor use the target's full content digest (the information from which its content UUID is derived). Unresolved free identifiers have an explicit `unresolved` tag and their spelling; structured paths retain ordered syntax and name tokens. Local bindings take precedence over reference substitution. Cross-file, overload and qualified resolution are deliberately unchanged from WP-22. Reordering an unrelated top-level item does not affect the requested identity.

### Observable spelling and supported syntax

| Language | Retained | Dropped or normalized | Conservative failures in v1 |
|---|---|---|---|
| Rust | literal spelling, operators, types, generic names, lifetime names, field/member names, visibility, non-doc attributes, mutability, macro token-tree spelling | comments and doc attributes/comments; value parameter names and supported local patterns | match/conditional/let-else binding scopes, closures, function-local imports, unsupported binding patterns, macro definitions |
| Python | keyword-capable parameter names, keyword argument labels, attributes, decorators, annotations, defaults and literal contents | comments and leading function/class docstrings; positional-only parameter names; ordinary function-local assignments | nested functions/classes inside functions, comprehensions/generators, lambdas, global/nonlocal, augmented/walrus bindings, for/with/exception/match/delete bindings and function-local imports |
| Julia | field names, types, generic/where names, literal contents, operators and quoted syntax | comments and attached doc strings outside occurrences; positional parameters and ordinary function-local assignments | keyword parameter declarations, nested functions, arrow/do closures, comprehensions, explicit local declarations, for/let/catch scopes, macro definitions and unsupported patterns |

Macro token trees and quoted/string data are opaque and preserve their spelling, including interior whitespace. Ordinary formatting outside opaque data is ignored. Python parameter renames that change keyword calling conventions **must** change the id; §9's renamed-local test does not require them to be equal. Documentation is deliberately excluded even where introspection can observe it. Attributes/decorators and observable local spellings can therefore make two otherwise similar definitions differ. A future extension to these rules must increment the normalizer version; supported syntax must not silently become a fallback approximation.

### Recursive components

Iterative SCC discovery orders components dependency-first. Within an SCC, references are ordered ports determined by the syntax stream. For each possible root, first-visit traversal assigns member indices; the lexicographically smallest complete component serialization is canonical. Its BLAKE3 digest is the component digest. A member digest hashes tagged component digest and u32 canonical index. External references include the already-computed target digest.

If several roots give the same minimal serialization, each member uses its smallest index across those roots. Automorphic, structurally indistinguishable members may share a content id. This resolves the draft's previously unspecified symmetric-recursion tie without introducing a name/path/order dependency. Different ordered wiring remains distinguishable. Root candidates share the request work budget rather than performing unbounded graph-isomorphism search.

### Cache, freshness and bounds

Parsing/hashing and optional synchronous state-store access run through a blocking worker, outside the UI input path. Memory LRU budgets are 16 MiB per index on desktop/server and 4 MiB on Android. `with_content_id_store(Arc<dyn StateStore>)` optionally stores rebuildable identity records in a versioned prefix of the existing INDEX table, with the same byte budget and LRU timestamps. Each record is at most 2 MiB and checksummed; malformed, incompatible or failed cache reads are misses. Records contain ids/digests and metadata, never copied source text. Full index persistence remains WP-21; platform constructors do not install this optional cache themselves.

The key includes normalizer version, exact BLAKE3 file-input hash and source/location selector. Since resolution remains same-file, that exact input covers every resolved dependency; a changed dependency cannot hit an old result. These location-scoped cache keys do not enter content hashes. Persisted cache byte enforcement is serialized within this process; separate processes may temporarily exceed the budget until a later write evicts entries. Cache failures never block recomputation.

Limits: 512 KiB per file, 4,096 definitions per input file and reachable closure, 32 MiB aggregate canonical syntax, depth 128, one million syntax/graph canonicalization steps, and a two-second parser progress deadline. Only one file is fetched per request (then fetched again for freshness), so the 32 MiB fetch allowance is not approached. Resource exhaustion, malformed/unsupported syntax, stale input, unavailable source and failed worker produce typed errors with no partial publication.

Before computing, fetched bytes must match indexed provenance. Before publishing, bytes are fetched again, the index generation is checked, and every affected node must still exist. Refresh removes previously published identities for that file; deletion removes its definitions. No old id is published over a concurrently refreshed graph. Unrefreshed source changes can still make an already-published graph snapshot stale until the existing watcher/refresh path runs; this feature adds no new watcher guarantee.

Three-language integration fixtures cover moves/renames/trivia, body and dependency changes, recursion, unresolved references, observable spelling, lexical shadowing, optional persistent reopen, refresh/deletion, concurrent refresh and parser limits. Unit tests cover ordered wiring, disconnected components, canonicalization exhaustion, byte-budget eviction and invalid cache records. See [[P-200 Lazy syntactic content identities]] and `packages/services/index/index.md`.


## WP-67 slices 1–5 (2026-10-16)

The native, default-off `lossless-syntax` feature now provides exact Rust token/trivia snapshots and pure guarded declaration-name/move plans. `SourceDescriptor.truth` is built with backwards-compatible `Files` deserialization and explicit `Store` metadata; existing sources remain externally authoritative. Default-off `structural-edits` now exposes declaration-name and same-file function moves through a revision-aware document adapter and graph preview/conflict UI. Reference-aware graph Rename now uses complete same-folder rust-analyzer plans, coordinated buffer history and explicit per-file saving; cross-file moves remain unimplemented. See [[Rust Lossless Syntax and Patches]] for the byte/trivia ownership policy, bounds, move-token conservation and the document adapter and experimental graph controls. Content identities and exact input hashes have different purposes; a doc-only change preserves the former but invalidates an old structural edit plan.
