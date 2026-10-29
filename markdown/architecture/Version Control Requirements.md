---
title: "Version control — the full frontend requirement list"
description: Everything Moonkale's frontend should do for git and other version control systems (Jujutsu, Mercurial, Pijul, Fossil, data and database versioning), what is built, what is Moonkale-specific (semantic diffs, history of a node, agents' changes as branches, versioned sessions), and what is rejected because it would tie Moonkale to one system. 2026-10-14.
tags: [architecture, version-control, git, requirements]
---
**Question (Daniel, 2026-10-14):** do we already have the full frontend requirement list for git and other version management systems? **Not until this page.**
- [[Version Management]] decides the model: two histories, git for files and the entity log for the graph, bridged by checkpoints.
- The git extension (Milestone 7) has status, diff, stage, commit and history as a graph, over the `git` CLI.
- There was no requirement list beyond that, and nothing for systems other than git.

Requirements below are `VC-nn`; ✅ marks what is built.

## The principle

**Moonkale is not a git client with extras. It is a graph editor that respects whatever versions its sources.** So the frontend talks to a **version-control trait** (status, diff, record, history, branches, sync, conflicts), with git as the first implementation, not the definition. Every view below works on that trait. A system that lacks a feature says so, and the UI hides the action rather than faking it.

## Everyday work (git first)

- **VC-01 Status:** changed, added, deleted, renamed, untracked and ignored files, in the Explorer and the editor gutter. ✅ Status and gutter marks; renames detected; ignored files shown on request.
- **VC-02 Diff:** inline and side by side, word-level within lines, whitespace toggle, against the index, HEAD, any commit or branch. ✅ A diff panel; side-by-side and word-level open.
- **VC-03 Stage what you mean:** whole files, hunks, single lines, and unstaging the same way. ✅ Files; hunks and lines open.
- **VC-04 Commit:** a message editor with a subject-line ruler, amend, sign-off and co-author trailers, signing (GPG or SSH), and the hooks run with their output shown. ✅ Basic commit.
- **VC-05 Branches:** create, switch, rename, delete, set upstream, compare two branches; dirty-tree checks before switching, with an offer to stash.
- **VC-06 Sync:** fetch, pull (merge or rebase, as configured), push, push with lease, and publishing a new branch. **Credentials** come from the system (ssh-agent, the git credential helper), never stored by Moonkale ([[Security]]). Progress and errors are shown verbatim. ✅ WP-51 (2026-10-29): reviewed server-rechecked plans, configured pull, exact-commit leases and publishing; local bare-remote lifecycle and browser acceptance.
- **VC-07 Conflicts:** merge, rebase and cherry-pick conflicts in a three-way view (ours, theirs, base, result). Pick per hunk, edit the result, mark resolved, continue or abort. ✅ WP-51 (2026-10-29), for bounded text conflicts: reusable three-way model/component, per-hunk choices, editable result preserving existing partial resolutions, explicit deletion resolution, stale-write refusal and merge/rebase/cherry-pick continue/abort. Binary and file-mode resolution still use external Git. The component is available for [[Shared Work]] SW-21; its proposal adapter remains WP-49.
- **VC-08 History:** the log ✅ (as a graph, too: commits as nodes); per file; **per symbol or node** (see VC-20); with search by message, author, path and content (pickaxe).
- **VC-09 Blame:** per line, in the gutter and on hover, with "the commit before this one" to walk back.
- **VC-10 Undo for git:** revert a commit, reset (soft or mixed; hard only with confirmation), cherry-pick, and the **reflog** as a recovery view ("what did I lose?").
- **VC-11 Stash:** push (with untracked), list, show, apply, pop, drop.
- **VC-12 Tags and releases:** create (annotated or signed), push, list; a release's notes from its commits.
- **VC-13 The rest of git:** worktrees (this project lives on them), submodules, LFS, sparse checkout, bisect (good/bad marks, the culprit shown), `.gitignore` editing from the Explorer, and the repository's config shown read-only.

## Hosts: GitHub, GitLab, Codeberg

- **VC-14 Pull/merge requests:** list, open, check out, review with comments as annotations ([[Annotations]], [[Shared Work]] §4), approve, merge. A PR is a proposal (SW-23) that lives on a host.
- **VC-15 Checks:** CI status on commits and branches, and failing logs opened as a run record.
- **VC-16 Issues** as annotations anchored to files and lines ([[Annotations]], "Issues from elsewhere").

## What only Moonkale does

- **VC-20 History of a node, not a path:** a function's history across renames, moves and file splits, through the index's symbols and, later, content-addressed identity ([[North Star]]). It is the same for a flow block, a table, a note's section.
- **VC-21 Semantic diffs, per kind of document:**
  - a flow diff (blocks added, wires changed, parameters edited, drawn on the canvas);
  - a notebook diff (cells, outputs);
  - a table or CSV diff (rows and cells);
  - an image diff (side by side, onion skin);
  - a graph diff (nodes and edges in, out, changed, coloured on the graph);
  - a Markdown diff on rendered text.

  A line diff stays available for all of them.
- **VC-22 The two histories meet:** a commit is a checkpoint in the entity log ([[Version Management]]), so time travel crosses it in both directions. "Which commit captured this edit" and "what did the graph look like at that commit" each take one click.
- **VC-23 Agents work on branches or proposals:** an agent's change set is a branch (or a jj change) or a proposal (SW-23), never a stream of commits on your branch. Review is a diff view, and accepting it is a merge.
- **VC-24 Sessions at a revision:** a session can pin a source to a commit or branch ([[ADR-0025 Sessions are the outermost state]], an open question there). This is for reproducible runs (Lenticulum's run records name their model version, LT-13) and for reviewing an old state without checking it out over your work.
- **VC-25 Versions of things that aren't files:** a database source versions what its engine offers:
  - Dolt's branches and commits;
  - Neon's branches;
  - lakeFS for object stores;
  - TerminusDB;
  - tiramemsu's transaction time and oxilite's versioning (as-of reads).

  Where the engine has nothing, the entity log records what Moonkale saw (the existing rule).

## Other version control systems

| System | Why it matters | Verdict |
|---|---|---|
| **Jujutsu (jj)** | the working copy is a commit; an operation log with undo of anything; conflicts are recorded, not blocking; works on git repositories | **First after git.** Its model (changes, not a staging area; conflicts as data) fits proposals and agents' work best |
| **Mercurial, Sapling** | used in large monorepos; Sapling has stacks of commits as a first-class workflow | through the trait, later; stacks inform VC-23's UI |
| **Pijul** | patches commute; history as a graph of patches | interesting for the graph model; later, as research |
| **Fossil** | VCS with built-in tickets and wiki in one SQLite file | later; its "everything in one database" is close to Moonkale's state store |
| **Subversion, Perforce** | centralised; large binaries, games | only if a user needs them |
| **Data versioning:** DVC, git-annex, LFS, lakeFS | large data files beside code (omics, market data caches) | LFS with git (VC-13); DVC as an extension when a user needs it |
| **Unison-style content addressing** | code identified by hash; names are metadata | the long-term model ([[North Star]]), not a VCS integration |

## Rejected or limited

The [[Scrutiny]] entries:
- **Assuming git everywhere:** a frontend that calls git directly from views ties every feature to one system. Views go through the trait.
- **Line diffs as the only diff:** they are wrong for flows, notebooks, tables and graphs (VC-21).
- **Scraping CLI output as the long-term interface:** fine for now (it's what the git extension does, and it matches the user's git exactly), but parsing human output breaks. Use porcelain formats (`--porcelain=v2`, `-z`) everywhere, and move to `gix` where it covers the feature.
- **Storing credentials in Moonkale:** the system's agents and helpers own them.

## Order (proposal)

1. **Sync and conflicts** (VC-06, VC-07): without push and pull, the git extension can't be someone's only git tool.
2. **Hunks and lines, blame, stash, branches** (VC-03, 05, 09, 11).
3. **History of a node and semantic diffs for flows and tables** (VC-20, VC-21): Moonkale's own value.
4. **The trait, then jj** (VC-23 rides on it).
5. **Hosts:** PRs as proposals, checks, issues as annotations (VC-14 to 16).

Work packages WP-51 to WP-53 in [[Work Packages]].
