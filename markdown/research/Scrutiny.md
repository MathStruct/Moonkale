---
title: "Scrutiny — what we did not take from prior art, and why"
description: Specs and design decisions found in other projects that Moonkale rejects or limits, each with the reason, so a later reader neither re-imports them by accident nor loses what they offered.
tags: [research, specs, prior-art, moc]
---
When Moonkale studies another project for requirements (Recoco and Thread, the Supabase table editor, Obsidian's canvas, ModelingToolkit's tooling, …), most of what it finds becomes Moonkale's own spec. Some of it does not. A behaviour may fit that project's constraints (one database, a hosted service, a browser-only client) but would **limit Moonkale**: tie it to one backend, one platform or one storage model, or close off the factor-graph and multi-source goals ([[Julia and Lenticulum]], [[Projects and Sources]]). Some of it is simply a weak decision.

Those land here, not in the spec: one entry per item, with what it was, why it is out, and what Moonkale does instead. An entry can be overturned. It then moves to a spec with a note why the reason no longer holds.

## Rules for a study

1. **Requirements and behaviour, never code.** We write down what a tool *does* and why, from its documentation, its UI and its public behaviour, then specify and implement it in Rust for Moonkale. We copy no code, schema, CSS or text, whatever the licence. A permissive licence (MIT, Apache-2.0) would allow reuse with attribution, but that needs its own decision; a copyleft one (AGPL, GPL) never.
2. **No names or branding.** Moonkale's features are named for what they do ("schema view", not another product's name).
3. **Every requirement is judged on its own:** taken (into a Moonkale spec), adapted (taken with a change, the change noted in the spec), or **rejected or limited** (here).
4. **The test for "limits Moonkale":**
   - Does it assume one backend, one platform, a server, or a network?
   - Does it lose data or history that Moonkale keeps?
   - Does it make an extension point narrower?
   - Does it conflict with a decision already taken (an ADR)?
   - Does it trade correctness for a convenience Moonkale can get another way?

## Entry format

```
### <source> — <the requirement or decision, in a line>
- **What it is:** the behaviour, as the source has it.
- **Why not (or not as is):** the reason, against the test above.
- **Instead:** what Moonkale does, with a link to the spec or ADR.
- **Revisit when:** the condition that would change the verdict.
```

## Recoco and Thread (2026-10-12)

The study is [[External Code Intelligence Projects]]; its section "What should not transfer" is the list. In short:
- a dataflow engine in `moonkale-core`;
- Thread's storage stack (Postgres, D1, Qdrant, Vectorize);
- base-plus-local-delta overlays before there is a multi-user requirement;
- their performance targets as promises.

## Supabase table editor (planned)

Not studied yet. The study writes its taken and adapted requirements into the [[Table Editor]] spec, and its rejections here.
