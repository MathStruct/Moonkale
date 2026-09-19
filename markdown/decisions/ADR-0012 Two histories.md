---
title: "ADR-0012 — Two histories: git for files, an entity log for the graph"
tags: [adr, versioning]
status: accepted
date: 2026-09-17
---
**Status:** accepted (Milestone 8, 2026-09-19) · Overview: [[Version Management]] · Prior art: [[Versioning Prior Art]]

## Context
Users will keep code in git and expect Moonkale to respect it (status, diff, commit). But most of what Moonkale adds — nodes derived from databases, wiki-links, agent-made changes, cross-source edges — has no file to diff. Git's unit of history is a text diff keyed by path; the graph's natural unit is *entity added / entity removed*, keyed by UUID, with a timestamp. Three options were considered:

| option | how | verdict |
|---|---|---|
| A. Git for everything | serialise the graph to files and commit | ❌ renames become churn, databases can't be committed, agent changes need a working tree, merges are textual |
| B. Entity log for everything, git ignored | our log is the only history; git is just a folder | ❌ breaks every user's workflow; history of code must stay in git |
| C. **Two histories, one seam** | git owns text history of folder sources; an append-only entity log owns graph history; checkpoints tie them together | ✅ |

## Decision (accepted — built in [[Milestone 8 - Implementation Log]]; the implemented shape is `core::graph::history`: `Event { id, at, actor, key, cause, kind }`, `EventKind::{Add, Remove, Rename, Content{patch, base}, Checkpoint}`, JSONL in `.moonkale/history.jsonl`; git checkpoints are recorded from the Changes panel; time travel *across* git — restoring a state — is not built yet)
1. **Entity log** in `core::graph::history`: events `Add / Remove / SetProps / Content` on UUID-identified entities, with timestamp, actor, cause and transaction id; state is a fold; tombstones instead of deletion; snapshots as an optimisation. `Version` becomes "id of the last event that touched this node".
2. **Git stays authoritative for text** in folder sources. A `vcs-git` extension (native; server-side on web) provides status, diff, commit, log, and renders history as a graph.
3. **Checkpoints** link the two: a commit records the log events it captured; time travel crosses the boundary in both directions.
4. **Content conflicts** are delegated to text patches now and a CRDT later ([[ADR-0009 Patches not snapshots]]); structural changes (add/remove) merge by set semantics with an explicit add-wins policy.
5. **Agents write through the log** like everyone else; the audit log and the history are one thing.

## Consequences
- The model gains `Add/Remove` ops it lacks today, and every source's write path must append to the log atomically.
- Storage and compaction policy become real work (retention window, never compact behind a checkpoint).
- Sync between devices becomes possible without a server (logs merge), but needs logical clocks and actor ids from day one — cheap to add now, expensive later.
- Undo/redo falls out of the log (`cause` links), replacing any per-editor undo stack for structural edits.
- Nothing in Milestone 1 changes immediately; this ADR fixes the *shape* before the index (Phase 2) starts emitting derived nodes that would otherwise have no history.
