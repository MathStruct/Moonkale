---
title: "Annotations — comments on anything, and issues from elsewhere"
description: A plan for comments and annotations attached to places in files and sources (a line range, a table row, a graph node, a document) — threaded, resolvable, stored with the folder, shown in gutters and a panel and as graph nodes — and for importing GitHub/GitLab issues and review comments as the same thing. Not built.
tags: [architecture, collaboration, design]
---
From [[Prompt19]] (2026-09-20): plan for annotations/comments that can occur in files and sources — "like issues in GitHub" — for later. Related: [[Collaboration]] (presence, history), [[Projects and Sources]] (git-host repositories as sources), [[Graph-Native Model]].

## What an annotation is
A small document anchored to a place:

```jsonc
{ "id": "…", "anchor": { "source": "folder:…", "node": "<node id>", "range": { "start": { "line": 12, "col": 0 }, "end": { "line": 14, "col": 40 } } },
  "author": "Daniel", "at": "2026-09-20T10:00:00Z", "text": "this branch is unreachable since …",
  "thread": "<root id or null>", "state": "open" | "resolved", "labels": ["bug"],
  "origin": { "kind": "local" } | { "kind": "github", "repo": "MathStruct/Moonkale", "number": 42, "url": "…" } }
```
- **Anchors** are the graph's own addresses: a node (file, table row, graph vertex, page) plus an optional range inside its text. Because the entity log already tracks every edit ([[ADR-0012 Two histories]]), an anchor survives edits: it is re-mapped through the log's patches like presence cursors and history are, and an anchor whose text moved is shown as *drifted* with the original quote kept.
- **Threads**: replies reference a root; a thread has one state (open/resolved).
- **Storage**: `.moonkale/annotations.jsonl` in the folder — append-only like the history file, one JSON per line, merged by id (so it syncs through git or any folder sync and conflicts resolve by timestamp); for non-folder sources (a database) the annotations live in the *project* ([[Projects and Sources]]) keyed by the source id.
- **Annotations are nodes** in the graph (`NodeKind::Annotation`, edges `Annotates` to the anchor), so the graph view shows them, search finds them, the agent can read them (and, under policy, create them), and the History panel shows when they were made.

## Where they appear
- **Editor gutter**: a marker on annotated lines (code and markdown source); a highlight over the range; a side popover with the thread; *Add comment* on a selection (context menu, `Ctrl+Alt+M`).
- **Rich markdown**: the same as a margin note.
- **Table editor**: a marker on annotated rows/cells.
- **Graph**: a small badge on annotated nodes; annotation nodes toggleable as a kind filter.
- **Annotations panel** (activity bar entry — [[009]]): open/resolved, by file, by author, by label; mine/others; click → jump to the anchor.
- **Status bar**: count of open threads in the active document.

## Issues from elsewhere
The git-host repository source ([[Projects and Sources]]) brings **issues and pull-request review comments** in as annotations with `origin.kind = "github" | "gitlab" | "codeberg"`: an issue anchors to the repository (or to a file/line when its body references one, or a review comment to its diff hunk mapped to the current line through git blame); replies and state changes go back to the host through its API with the token the source already has; local-only threads never leave the machine unless the user *publishes* one as an issue. Sync is per source, on open and on demand, with the host's `updated_at` as the clock.

## Who writes them
Daniel, others through presence sessions (the hub relays new annotations to the room like presence), and **the agent** — an annotation is the natural place for an agent's remark ("this query has no index") that should not become an edit; policy classifies creating one as *Mutating* but non-destructive.

## Not decided
Whether an annotation may anchor to a *version* (a commit) rather than the live text; whether resolved threads are trimmed from the JSONL on compaction like history; the exact drift rule when the anchored text is deleted (keep, orphan, or resolve automatically).

## Steps (when scheduled)
1. `core`: `Annotation` type, `NodeKind::Annotation`, `EdgeKind::Annotates`; `ext-api`: `Workspace::{annotations, add_annotation, resolve, reply}` over `.moonkale/annotations.jsonl` with anchor re-mapping through the entity log.
2. Gutter markers + popover in the code editor; the panel; status count.
3. Rich markdown and table markers; graph badges and annotation nodes.
4. GitHub import/export through the repository source.
