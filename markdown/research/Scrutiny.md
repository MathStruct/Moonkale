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

## oxilite studio (2026-10-14)

The study is [[Knowledge Graph Studio]]. Taken: almost everything. Rejected or limited:

### oxilite studio — the graph view caps at 1 500 nodes
- **What it is:** the node-link model is built and drawn up to 1 500 nodes; the rest are counted.
- **Why not:** a cap of that size fits Cytoscape in a webview, and Moonkale's engine draws 100 k. A cap here would hide exactly the large ontologies and memories the studio is for.
- **Instead:** the graph engine, with level of detail; the coarse tier (WP-06) past that.
- **Revisit when:** never for the cap itself; the engine's limits are the bar.

### oxilite studio — results are cut at a row limit
- **What it is:** rows beyond `oxilite.query.rowLimit` are dropped, and server paging is listed as still to come.
- **Why not (as is):** a silent cap loses data in the one view meant to show it.
- **Instead:** the virtualised table grid holds the whole result up to the source's cap and says when the source truncated; pushdown paging is WP-13. Like tiramemsu's budgets, an over-budget answer is an error, never a quiet truncation.
- **Revisit when:** —

### oxilite studio — VS Code webview workarounds
- **What it is:** a mailbox that holds a panel's last message until the webview is ready, and link-styled buttons instead of `<a>`, because notebook hosts intercept links.
- **Why not:** they work around VS Code's webview hosting, which Moonkale doesn't have. Moonkale renders Dioxus components in its own tree.
- **Instead:** nothing needed.
- **Revisit when:** a Moonkale view is hosted in an iframe or a webview of its own.

## n8n and Windmill (2026-10-14)

The study is [[Workflow Automation (n8n, Windmill)]]. Licences: n8n's Sustainable Use License (source-available) and Windmill's AGPLv3 allow requirements only. Rejected or limited, each with its reason there:
- the integration catalogue as a goal;
- untyped JSON items with field-mapping expressions;
- a hosted multi-tenant runner;
- auto-generated end-user apps and forms.

## Debuggers and plotting tools (2026-10-14)

From [[Debugger Requirements]]:
- VS Code's `launch.json` substitution language as is;
- a debugger that blocks the UI.

From [[Plotting Requirements]]:
- Tableau's proprietary formats and server model;
- a JS charting library in the core;
- pixels as the stored artefact.

## Obsidian, VS Code, Zed (2026-10-14)

The study is [[Editor Prior Art (Obsidian, VS Code, Zed)]]. Rejected because it would limit Moonkale:
- **Each tool's founding assumption:**
  - Obsidian's "files are the only truth";
  - VS Code's "every view is about a text document";
  - Zed's "a worktree of files is the project".

  Moonkale's is "everything is a node, from any source".
- **Obsidian plugins with full app access and no permissions.** VS Code's Node extension host and iframe webviews.
- **Zed's rule that extensions add no UI,** which would forbid Moonkale's panels.
- **Sync, Publish, Live Share and Settings Sync as vendor services,** and assistants locked to one vendor.

## Trading with Databento (2026-10-14)

From [[Trading with Databento]]:
- order entry and execution in Moonkale;
- execution-grade UI latency as a goal;
- republishing licensed market data (a source can mark its data non-redistributable, and publishing refuses it).

## Version control (2026-10-14)

From [[Version Control Requirements]]:
- assuming git everywhere (views go through a version-control trait);
- line diffs as the only diff;
- scraping human CLI output as the long-term interface (porcelain formats now, `gix` later);
- storing credentials in Moonkale.

## Supabase table editor (planned)

Not studied yet. The study writes its taken and adapted requirements into the [[Table Editor]] spec, and its rejections here.
