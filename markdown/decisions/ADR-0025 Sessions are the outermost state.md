---
title: "ADR-0025 — Sessions are the outermost state, kept in the state store"
tags: [adr, sessions, projects, state, sources]
status: accepted
date: 2026-10-14
---
**Status:** accepted 2026-10-14 (Daniel), the same day it was proposed; written by Claude. It supersedes the *storage* part of [[Projects and Sources]] (one JSON file per project) and builds on [[ADR-0014 One store for internal state]] (SQLite). Related: [[Internal State]], [[Agent Sessions and Profiles]], [[Collaboration]].

## Context

- **Several sources, not one folder.** Moonkale opens several sources at once, not a single folder ([[Projects and Sources]], spec [[003]]). That design calls the saved set of sources a **project** and stores it as `~/.config/moonkale/projects/<id>.json`.
- **Daniel (2026-10-14):** the outermost way to manage different projects should be **sessions**. People switch between Moonkale sessions, which are not linked to a folder and keep the work done in them. And, as decided before, Moonkale's state goes into a **database, not file dumps**. The state store exists (SQLite, ADR-0014): settings, layouts, the entity log and agent sessions moved in on 2026-10-02.
- **Today:** state is keyed **per folder**: layout and open documents per folder and machine, agent sessions per folder on its host. With several sources open at once, "which folder does this layout belong to" stops having an answer.

## Decision

1. **A session is the outermost unit of Moonkale state.** It is named and persistent, and **not bound to a folder or any source**. It holds:
   - **Its sources:** which belong to it, each open or closed, read-only or not (a property of the pair *(source, session)*, as the Projects design has it), with its colour. Sources are members of sessions, never the other way round, and one source can belong to several sessions.
   - **The workspace:** the panel layout, open documents and the active one, each panel's view state (a graph's camera, a table's query and scroll, the flow editor's open scope), and **unsaved buffers**. Switching away never loses an edit.
   - **The work done in it:** agent conversations, query history, search history, run records (WP-38), notebooks' kernels, and scratch notes that belong to no source.
   - **Its settings:** overrides in the scope order `user < session < folder`. The folder scope stays data, not authority ([[Security]]).
   - **Never a secret:** only secret names, resolved per machine.
2. **Sessions live in the state store,** never in files. They are rows in the user's `state.sqlite`, in tables such as `sessions`, `session_sources`, and `session_state` (a key per panel and per kind of state). On a server they live per user, in the server's store.
   - Today's per-folder rows move under a session key: layout, open documents and agent sessions get a `session` column. The folder stays part of the key where the state really is about the folder.
   - The entity log stays **per source**: it records what happened to the source, whoever's session did it.
   - Export and import are explicit commands (a session bundle for another machine or a colleague), not the storage format.
3. **Switching is a first-class action.** The session switcher (palette, title bar, the phone's "More" sheet) lists sessions with their last use and members. Switching saves the current one atomically and restores the other: sources open or close, layout, documents, buffers and panels' state. Each window shows one session; two windows may show two.
4. **The Default session keeps today's behaviour.** Someone who never makes a session sees no change: "reopen the last folder" becomes "reopen the Default session". The existing per-folder rows migrate into it once.
5. **Operations:** new (empty, from the current one, or from a template), rename, duplicate, archive, delete (to a trash, restorable for a while), export, import (asking once for the secrets it names), and `moonkale --session <name>` on the command line.
6. **Failures are reported, never hidden:** a member source that's gone, or a remote that's unreachable, shows as such in the session and is never silently dropped (the Projects design's rule).

**"Project" and "session":** one concept, not two. The Projects page's *project* is this *session*; the page keeps its design for sources (open and close individually, read-only per pair, colours, failure notices, roots for portable paths) and drops its JSON storage.

**The name collision, settled (Daniel, 2026-10-14):** the agent's sessions are now **conversations** in the UI (English, German „Unterhaltung", Chinese 对话), and **session** means only this outer unit. The code and the store keep `agent_sessions` until the sessions migration renames it. Terminal and SSH sessions keep their word: those are processes, not saved state.

**Several people and agents in one session** is the next layer: participants, roles, approvals, attribution. [[Shared Work]] has it.

## Consequences

- **Projects as files are dropped.** The Projects page's storage section is superseded, and the rest of it stands.
- **The store's tables gain a session key.** That is a migration (ADR-0014's migration path), done once, with the Default session as the target of every existing row.
- **Sync of sessions between machines** waits for the store's sync (R-36), as everything in the store does. Until then, export and import carry a session between machines.
- **Collaboration:** a shared server holds sessions per user. A session can later be shared read-only, a natural place for [[Collaboration]] to start.
- **Not decided:**
  - whether a session can *pin* a source at a git revision (a reproducible session);
  - how long deleted sessions stay in the trash;
  - whether run records belong to the session or to the source they ran on (proposed: the session, with a link to the source).

Work package: WP-46 in [[Work Packages]].
