---
title: "Shared work — several people and agents in one session, on one graph"
description: Requirements for sessions with many participants (people and agents), output that renders as rich text, concurrent work on the same graph (what the database guarantees and what Moonkale must add), and comments on anything. From Daniel, 2026-10-14. Partly built (conversations render Markdown); the rest is design.
tags: [architecture, collaboration, sessions, agents, comments, requirements]
---
**From Daniel (2026-10-14):**
- Assume several people and agents work in one session, and that needs management.
- What agents and people output should render in rich mode.
- Several agents and people work on the same graph. He doesn't know the requirements exactly, nor how far that is Moonkale's concern as the frontend versus the database's.
- Annotating things with comments would be nice.

Builds on [[ADR-0025 Sessions are the outermost state]], [[Collaboration]] (presence today, CRDT editing later), [[Annotations]] (comments, designed), [[Agent Sessions and Profiles]] (now *conversations*) and the [[Security]] rules.

## 1. Participants: people and agents in a session

`SW-nn`.

- **SW-01 Participants with roles.** A session lists who takes part:
  - **people**, as owner, editor, commenter or viewer;
  - **agents**, each a saved agent (model and profile) with a **sponsor**: the person on whose behalf it acts, and whose permissions it can never exceed.
- **SW-02 Identity needs accounts.** A shared session needs to know who is who, which a single server token can't say. Accounts and roles on the server ([[Remote and Server Modes]], not built) come first. On one desktop with one person, every participant but the agents is that person.
- **SW-03 Presence includes agents.** Who is here, what they have open, where their cursor is (it exists for people), extended to agents: what an agent is reading or editing now, shown like a person's cursor, with its sponsor's colour.
- **SW-04 Tasks for agents.** An agent gets a task inside the session, **scoped** (these sources, this subgraph, read-only or with proposals). Tasks queue visibly with their state (waiting, working, needs approval, done), and a person can pause, redirect or stop one.
- **SW-05 Approvals have an inbox.** An agent's writes wait for approval (the policy gate). With several agents, they collect in one place per session: what each wants to change, as a reviewable **proposal** (SW-23), approved or rejected whole or in part, by anyone whose role allows it.
- **SW-06 Every change is attributed:** who did it, and for an agent also on whose behalf and under which task. The entity log already records who, when and what; it gains the agent-and-sponsor pair.
- **SW-07 An activity feed per session:** what happened while you were away (edits, approvals, finished tasks, new comments), each linking to its place.
- **SW-08 Budgets per agent:** tokens, time, number of writes, sources it may touch. Over budget, it stops and asks.

## 2. Output that renders as rich text

**Built (2026-10-14):** conversation messages, from people and agents alike, render as Markdown (`editors/agent/src/markdown.rs`): headings, lists, tables, code blocks, emphasis, task lists, links. It is safe by construction:
- raw HTML is shown as text;
- links keep their target only for http(s), mailto and relative paths;
- **images are never loaded**, because a remote image URL can carry data out (prompt injection, [[Security]]), so they show as links the user can open;
- math is kept as styled source.

**Next (SW-11 to SW-15):**
- **SW-11** math through the shared renderer ([[Math Delimiters and Renderers]]);
- **SW-12** wiki-links in messages open their node;
- **SW-13** code blocks highlighted with the code view's grammars;
- **SW-14** a proposed change renders as a diff;
- **SW-15** the same renderer for comments (§4), run outputs and task summaries, so everything people and agents write reads alike.

## 3. Many people and agents on one graph: the database's job and Moonkale's

The short answer to "how far is it Moonkale's concern": **the database guarantees that each write is correct; Moonkale makes sure people see each other, see conflicts, and can review what agents do. For sources with no database behind them (files, flow documents), Moonkale supplies the guarantees itself.**

| Concern | The database (when there is one) | Moonkale |
|---|---|---|
| A write is all or nothing | transactions | groups a proposal's edits into one transaction; for files, writes atomically (temp file and rename, the jail) |
| Two writers don't corrupt each other | isolation (locks, MVCC) | — |
| Rules hold (types, keys, cardinality, shapes) | constraints, triggers, SHACL (oxilite), predicate schemas (tiramemsu) | shows the refusal at the place it applies |
| Someone changed it since I read it | row versions, or conflict errors | **optimistic writes:** every write names the version it read (SW-20); a mismatch is shown, never silently overwritten |
| History of every change | only some keep one (tiramemsu never forgets; oxilite has versioning; Postgres doesn't) | the entity log for every source, with attribution (SW-06) |
| Who is working on what, right now | — | presence, soft claims (SW-22) |
| Seeing a conflict and choosing | — | a conflict view: both sides, who and when, keep one, merge, or reopen (SW-21) |
| Reviewing an agent's changes before they land | — | proposals (SW-23) |
| Undo *my* change, not yours | — | per-actor undo through the entity log or CRDT (SW-24) |
| Working on a copy, merging later | branching where the store has it (git for files, some databases) | scenario copies (LT-14) and proposals where it doesn't |
| Text and flow documents edited at once | — | a CRDT per document ([[Collaboration]], Phase 4–5) |

**Requirements:**
- **SW-20 Optimistic writes.** Every write through the `Source` write API (R-41, not built) carries the version it was based on. The source refuses a stale one, and Moonkale shows why. Last-writer-wins is never silent.
- **SW-21 A conflict view** with both sides, their authors and times, and the choices. It is the same view for a cell, a node's property, a flow block's parameter and a file.
- **SW-22 Soft claims, not locks.** "Daniel is editing this subgraph" or "agent X works on these files" is shown to everyone, and others are warned before editing there, never blocked. Hard locks are left to the database when it has them.
- **SW-23 Proposals (changesets):** a set of edits across sources, made by an agent (always) or a person (optionally), reviewed as a whole with a diff per source, commented on (§4), and applied atomically where the source can (one transaction), or in order with a report where it can't. An agent's work is a proposal by default.
- **SW-24 Per-actor undo:** undo the last change *I* made, even after others changed things elsewhere, through the entity log (structure) or the CRDT (text).
- **SW-25 Remote changes arrive live:** `changes_since` already exists for folders; it extends to every source that can report changes, so a session never works on a stale picture without saying so.

## 4. Comments on anything

[[Annotations]] already designs comments: anchored to a node and an optional range, in threads, resolvable, labelled, imported from GitHub issues, and stored as nodes in the graph. What this request adds:
- **SW-30 Anchors everywhere people work:** text ranges, graph nodes and edges, flow blocks, table cells and rows, plot points, a proposal's hunk, a run's step.
- **SW-31 Comments render rich**, with the renderer of §2.
- **SW-32 Mentions:** `@person` notifies, and **`@agent` asks that agent to act** on the thread, as a task (SW-04) scoped to the anchor, under the gate, answering in the thread. It's the same pattern as replying to a review comment.
- **SW-33 Comments drive review:** a proposal is approved with its comments resolved, or sent back with them.
- **SW-34 Where comments live (to decide):** the [[Annotations]] design stores them in `.moonkale/annotations.jsonl` inside the folder, so they travel with git. Daniel prefers state in a database over file dumps. The proposed compromise:
  - comments live in the **state store of the source's host**, like the entity log;
  - an **export/import to a file** shares them through git for those who want it;
  - comments on databases and graph stores were never going to be in a file anyway.

## Order (proposal)

1. **Comments** (WP-50): the [[Annotations]] steps with SW-30, SW-31 and SW-34; useful alone, even single-user.
2. **Proposals and optimistic writes** (WP-49): SW-20, 21 and 23, together with writes to database sources (R-41); they make agents' work reviewable.
3. **Participants, roles and agent tasks** (WP-48): SW-01 to 08. It needs accounts on the server first.

Per-actor undo and claims come with the CRDT phase of [[Collaboration]].
