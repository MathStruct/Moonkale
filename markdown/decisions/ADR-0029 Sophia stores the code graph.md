---
title: "ADR-0029 — Sophia stores the code graph; files stay the truth"
tags: [adr, sophia, index, code-graph]
status: accepted
date: 2026-10-23
---
**Status:** accepted (Daniel, 2026-10-23: "Agree!")

## Context
Moonkale analyses code in `services/index`: tree-sitter definitions, calls and references for the core languages, content ids from `moonkale-content-id` (WP-65, WP-66, WP-22). The index lives in memory. WP-21 planned to make it persisted and incremental, based on the Recoco and Thread findings ([[External Code Intelligence Projects]]).

Sophia (`MathStruct/Sophia`, [[North Star]]) has grown into that store meanwhile: content-addressed definitions, names that move, a change feed, `sql` and `query` dialects, rendering profiles, and a code ingest. The ingest uses Moonkale's content-id crate, and the ids are equal to `NodeId::from_content` in Rust, Julia and Python. Moonkale reads it through `sources/sophia` (WP-68), behind the default-off `sophia` feature.

Daniel asked whether code analysis should move entirely to Sophia and become the default backend.

Against moving *entirely*:
- [[ADR-0028 Files or the graph]] keeps files as the truth for ordinary repositories, with lossless edits.
- Sophia is a store-is-truth system and reprints definitions in a profile, so a file round trip through it is not byte-exact.

Against making it the default *now*, from the week of 2026-10-20:
- P-229, a stack overflow in `Interface::all`, fixed within hours;
- SP-015, about 7% of generated definitions can't be saved back unchanged;
- deep definitions can still overflow `fetch_text` in debug builds;
- three revisions in three days and no tag;
- no wasm build yet. Moonkale's full web version is required.

## Decision
1. **Sophia is the code graph's store and query engine.** No separate persisted index (WP-21's Recoco/Thread direction is dropped).
2. **Files stay the truth** for file-backed sources (ADR-0028). Moonkale keeps parsing, lossless syntax and structural edits as text patches. The index becomes a producer: it writes definitions, content ids and edges into a Sophia store per opened folder (under `.moonkale/`). That store is a derived cache, rebuilt from the files when it is missing or stale.
3. Store-backed code (a `.sophia` file, `truth: Store`) keeps working as today through `sources/sophia`.
4. **The Sophia store must compile to wasm** (Daniel told the Sophia session, 2026-10-23), so the web client can use the same store. Until it does, the web version uses the server's store.
5. **Default only after these gates:**
   - a tagged Sophia release instead of a git revision;
   - SP-015 closed;
   - the wasm build working, or the server fallback in place;
   - the Moonkale repository itself measured over several days of normal work: index time, query times, no lost or stale facts.
   Until then the folder store sits behind the `sophia` feature, and the in-memory index stays the default.

## Consequences
- WP-21 is reworded: "the index writes into a Sophia store per folder; reopening reuses it; a one-file change rewrites only that file's facts". Its acceptance tests stay.
- The graph view, search and the agent read the folder store once it exists, through the `Source` trait. No Sophia types appear outside `sources/sophia` and the index's writer.
- The interface between the index and Sophia's ingest is agreed with the Sophia session: which records and edges, the normaliser version, incremental updates per file.
- `moonkale-content-id` stays shared. A normaliser change means a version bump on both sides, as with v7.
- Risk: Moonkale's code analysis depends on a young project in the same organisation. It's mitigated by the feature gate, the derived-cache rule (deleting the store loses nothing) and the default gates above.

Related: [[ADR-0028 Files or the graph]], [[Code Graph Schema]], [[Indexing]], [[North Star]], [[Work Packages]] (WP-21, WP-68).
