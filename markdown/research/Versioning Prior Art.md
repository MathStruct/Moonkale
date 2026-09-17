---
title: "Versioning Prior Art"
description: Systems whose version model is close to what the entity log needs, and what to borrow from each.
tags: [research, versioning]
---
Companion to [[Version Management]] and [[ADR-0012 Two histories]]. Descriptions are from public documentation; verify details before borrowing code.

| system | model | borrow |
|---|---|---|
| **git** | DAG of snapshot commits; diffs computed on demand; identity = path | the checkpoint idea; history-as-graph rendering; `gix` (pure Rust) / `git2` bindings |
| **Datomic** | immutable *facts* `(entity, attribute, value, tx, added?)`; database *as of* any transaction | the fold model exactly: our events are Datomic facts at entity granularity; "as-of" queries |
| **XTDB** | bitemporal: valid-time *and* transaction-time per fact | keep both timestamps — "when was it true" vs "when did we learn it" matters for imported database rows |
| **TerminusDB** | git-like branches/merges over an RDF graph, stored as deltas | branching a graph; delta storage; merge as set operations on triples |
| **Dolt** | git semantics (branch/diff/merge) for SQL tables, cell-level diffs | proves table history is tractable; a Dolt source could map its history to the log directly |
| **Unison** | code is content-addressed by hash; names are metadata; no diffs — definitions are added, never changed | the "append, never mutate" stance; renames as metadata; strong reason to treat a Unison codebase as a first-class source |
| **Automerge / Loro** (CRDTs) | per-replica operation logs with causal ordering; text, lists, maps merge automatically | the content layer under `Content` events; logical clocks (`loro` 1.x is Rust-native) |
| **Event sourcing** (general) | append-only events, state by projection, snapshots for speed | vocabulary and the snapshot/compaction discipline |
| **Fossil** | SCM with an append-only artifact store; "unversioned" vs versioned content | tombstones ("shunning") as a first-class, auditable operation |

## Take-aways for the design
1. **Append-only + fold** is well trodden (Datomic, event sourcing); the risks are storage growth and slow folds, both solved by snapshots.
2. **Two timestamps** (XTDB) cost little now and are painful to retrofit; include `valid_at` alongside `recorded_at` for imported facts.
3. **Structural merge is easy, content merge is hard** — every system above separates the two; so do we (entity log vs text patch/CRDT).
4. **Content addressing** (Unison, git objects) is the right key for *content*, UUIDs the right key for *identity*; Moonkale uses both: `NodeId` for the thing, a content hash for its text at a point in time.
