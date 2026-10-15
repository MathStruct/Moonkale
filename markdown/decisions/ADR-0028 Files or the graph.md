---
title: "ADR-0028 — Files or the graph: what is the truth for code"
tags: [adr, architecture, code-graph, versioning]
status: accepted
date: 2026-10-15
---
**Status:** accepted 2026-10-15 by Daniel: **option B**. Graph-side edits on file-backed code are in scope, language by language, Rust first. Sophia's storage is deferred (below). Claude wrote the options. Related: [[North Star]], [[Code Graph Schema]] (the draft that depends on this), [[ADR-0008 Rust owns the document, JS is a view]], [[ADR-0009 Patches not snapshots]], [[ADR-0012 Two histories]], [[Version Control Requirements]], [[Graph-Native Model]].

## Context

The [[North Star]] and today's code point in opposite directions.

**The North Star:**
- "Edit a graph whose text is a view."
- A definition is a node keyed by the hash of its structure, and source text is one projection of it.
- "Prefer the graph store's model over the file system's whenever the two disagree."
- "A file stays the import and export format for a language that lives in files."

**Today:**
- the code editor edits **text files**;
- git versions **files**;
- the index **derives** symbols from files;
- every outside tool (cargo, rust-analyzer, Julia, pytest, CI, GitHub) reads **files**.

The flow editor is the one exception: its diagram is the document. It's saved as its own file and never round-tripped through generated code (ADR-0020 to ADR-0022).

No page decides which one wins for *code*, and when. The answer changes nearly everything downstream:
- what an edit in a graph view does;
- what happens when a file changes on disk;
- what git sees;
- what collaboration merges;
- whether [[Code Graph Schema]]'s content ids are the identity or only an index.

**Three facts constrain the answer:**
1. **The ecosystems live in files.** Compilers, language servers, package managers, CI and code review read and write files. Unison is the one language that doesn't, and it ships its own tooling for everything as a result.
2. **Comments and formatting are part of what people wrote.** A graph that drops them can't print the file back the way its author left it. Keeping them means storing trivia (whitespace, comments, the exact token spelling) beside the structure.
3. **Other people edit the same files outside Moonkale.** That means another editor, `git pull`, a formatter or an agent CLI. Whatever is the truth has to absorb that.

## Options

### A. Files are the truth, always; the graph is derived

The graph (definitions, content ids, types, IR layers) is an index over the files. It's recomputed when a file changes, never edited directly. A graph view can *navigate* code, but an edit always opens the text.

- **For:**
  - it's today's model, and nothing breaks;
  - git and every tool keep working;
  - external edits are trivially right: the file wins.
- **Against:**
  - "edit a graph whose text is a view" never happens for code;
  - structure editors, projections ([[North Star]] §11, syntax as a user setting) and e-graph rewrites can't write back;
  - content ids become a cache key, not an identity, so history stays file diffs.
- **Cost:** none now.

### B. Per source: files stay the truth where code lives in files; graph-native content keeps its truth in a store; edits on file-backed code print back as text patches (recommended)

Each source declares where its truth lives:
- **`files`** for the languages of the ecosystems (Rust, Julia, Python, C/C++, …);
- **`store`** for content that has no file form, or is better without one: Sophia's definitions, a Unison codebase (WP-18), flows if they ever leave their files, annotations and proofs.

One graph spans both. Content ids are computed for file-backed definitions too, as a derived identity: a function moved between files keeps its content id.

**An edit in a graph view on file-backed code is allowed when it can print back.** The frontend keeps a *lossless* syntax tree (tree-sitter plus trivia: every comment and whitespace run attached to its token). A structural edit (rename, move, reorder parameters, a rewrite chosen in an e-graph view, a projection edit) becomes a **minimal text patch** on the file ([[ADR-0009 Patches not snapshots]]). Comments and formatting outside the changed region stay byte-identical. The file is still the truth: the patch goes through the normal document and save path, git sees an ordinary diff, and the graph re-derives from the saved file.

**External changes:**
- **File-backed:** the file wins, the graph re-derives, and an unsaved graph-side edit that no longer applies becomes a conflict shown to the user ([[Shared Work]]'s proposal model). It's never silently dropped.
- **Store-backed:** the store's own history merges it (a hash DAG, [[Version Management]]).

**History:**
- git for file-backed code, as [[ADR-0012 Two histories]] already says;
- the hash DAG for store-backed content;
- across both, content ids let history follow a definition through moves and renames.

**For and against:**
- **For:**
  - every ecosystem keeps working;
  - graph and projection editing become possible exactly where printing back is safe;
  - Sophia and Unison get a real graph-native home;
  - it extends today's architecture (sources, patches, two histories) rather than replacing it.
- **Against:**
  - two kinds of truth to explain;
  - a lossless printer per language (tree-sitter gives the tree; the trivia attachment and the patch printer are work per language);
  - an edit that can't print back (a projection that changes the meaning of the layout, for example) has to be refused for file-backed code.
- **Cost:** a lossless syntax layer and a patch printer per core language, started with Rust, Julia and Python. A `truth` field on the source descriptor. A conflict path for graph-side edits.

### C. The graph is the truth for everything; files are exported projections

Unison's model for all languages: code lives in the store; files are written out for tools and read back in when they change outside.

- **For:**
  - the North Star in full;
  - one identity, one history (a hash DAG), and moves and renames free;
  - projections and structure editing everywhere.
- **Against:**
  - every outside edit is an import with a merge, and `git pull` becomes "re-import and reconcile";
  - comments and formatting must round-trip through the store exactly, or every export rewrites files people share with others;
  - code review on GitHub sees exports, so they must be stable byte for byte;
  - tools that write files (formatters, `cargo fix`, agents) fight the store;
  - collaborators who don't use Moonkale see a project that something else keeps rewriting.
- **Cost:** very large and permanent: the importer and exporter per language, the merge, and the stability guarantees. It also makes Moonkale the owner of people's repositories, which they didn't ask for.

## Recommendation

**B.**
- It keeps the ecosystems' files and git as they are, which is what anyone outside Moonkale sees.
- It gives the North Star's graph editing a real path: the lossless syntax layer plus text patches.
- It gives graph-native code (Sophia, Unison, proofs) a store of its own.
- Option A is B without the printer. B can start as A, since the `truth` field defaults to `files` and graph-side edits are refused until a language's printer exists, so choosing B now costs nothing until the printer is built.
- Option C can be revisited per source later: a source can always declare `store`.

## Decision (Daniel, 2026-10-15)

1. **Option B.**
2. **Structural edits on file-backed code are in scope**: rename, move, rewrites from graph and e-graph views, and projection edits. They're done language by language, **Rust first**.
3. **Sophia's own definitions: deferred.** There are no real Sophia definitions yet, so where they live is decided once there are some. Daniel's alternative is to build a **base Sophia**, even a somewhat non-functional one, to find out what its definitions need. That's an optional work package (WP-68), not scheduled. The `truth: store` mode is still built, for the Unison source (WP-18), which needs it anyway.

## Consequences

- **The source descriptor gets `truth: files | store`.** Default `files`; every existing source is `files`.
- **[[Code Graph Schema]]:**
  - content ids are a derived identity for file-backed definitions and the primary identity for store-backed ones;
  - the lossless syntax layer (L1) becomes required for any language that allows graph-side edits.
- **New work packages:**
  - the lossless syntax layer and patch printer for Rust (then Julia, Python);
  - the `truth` field and the conflict path for graph-side edits;
  - store-backed definitions for Sophia (after WP-18).
- **[[Version Control Requirements]]:** unchanged for file-backed code. A "follow this definition" history view across moves, by content id, becomes possible.
- **Kept:**
  - ADR-0008 (Rust owns the document; a graph-side edit is a patch to that document);
  - ADR-0009 (patches);
  - ADR-0012 (two histories).
