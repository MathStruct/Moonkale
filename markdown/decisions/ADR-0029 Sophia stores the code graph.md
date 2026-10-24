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

**WP-21 implementation (2026-10-24):** the default-off folder writer and extra
read-only Source are implemented against Sophia `c3c48e02` (file manifest @1,
per-file fact replacement fixed). Reopen skips unchanged manifests, watcher
windows are atomic batches, normalizer changes rebuild, and stale reads fall
back to the current file. Definition ids and names rebuild from files; nominal
generic ids remain Sophia allocations. The tagged-release/default gates above
still apply; gate 4's measurement method is refined below. See [[Indexing]]
for verification, measurements and the initial module-naming limits.

**Interface (2026-10-23):** Sophia's SPEC-009 "Moonkale Folder Store" (Sophia rev 773d9898, accepted by Daniel 2026-10-23), which covers finished records per file, `Store::replace_files`, the enforced `store/mode = derived`, read-only SQLite for wasm, and `binding()`/`entity_path()` in `moonkale-content-id`. Daniel also cleared SP-015 and a v0.1.0 tag on those conditions; the Sophia work starts 2026-10-24.

Related: [[ADR-0028 Files or the graph]], [[Code Graph Schema]], [[Indexing]], [[North Star]], [[Work Packages]] (WP-21, WP-68).

**Gate 1 met (2026-10-24):** Sophia v0.1.0 is tagged (commit c3c48e02), and Moonkale pins `tag = "v0.1.0"` in `sources/sophia` and `services/index`. SP-015 is resolved too (Sophia cafe940e).

**Gate 4 methodology (2026-10-24):** replaced by a history replay, 2026-10-24,
because nobody yet uses the optional folder store in ordinary daily work and
cannot supply the requested days of use (Claude's decision, agreed by Daniel).
The original gate text above is retained. Real Git history is replayed as
watcher-shaped batches; every 50 commits and at the end, a fresh store of the
same tree is compared with the incremental one. Names, content ids, text-source
pointers, manifests, live generic memberships and facts are checked. Query
reads and batch times are measured, with no performance assertions.

**Gate 4 remains unmet:** the replay found lost/stale generic names and
memberships in both repositories ([[P-231 Sophia folder replay loses generic memberships]]).
Sophia v0.1.0 counts duplicate old generic member ids incorrectly during
replacement. Its Source also rejects the Search panel's `search` dialect;
rejection times are not successful search timings. See [[Indexing]] for the
complete runs, exact commits, measurements, limitations and verification.
This does not enable Sophia by default. A fixed tagged release and a clean
replay are still required.

**Gate 4 met at Sophia `77f5c1c9` (2026-10-24):** with Sophia's P-231 fix, the
replay of both histories (Moonkale 469 commits, Lenticulum.jl 130) has no
differences at any checkpoint; see [[Indexing]]. It counts once Moonkale pins
the fix by a tag (v0.1.1, waiting for Daniel), per gate 1. Search stays on the
in-memory index: Sophia has no `search` dialect, and adding one is a new
requirement for Daniel. Still open before the default switch: gate 3 (wasm read
path or the server fallback).
