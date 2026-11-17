---
title: "Backlog"
description: Where the open tasks are, how a task is picked and closed, and where finished tasks go. One task per agent session; the handoff to the next session is the top section of Status.
tags: [planning, agents, moc]
---
**Open tasks** are the packages in [[Work Packages]] (one row each: what, touches, depends on, size, done when). Pick the task named under "Next session" in [[Status]]; otherwise the first open package in your lane whose dependencies are done. A request from Daniel, a problem or a review that asks for new work gets a row in its lane there, with a number from the reservation table.

**Finished tasks** move from Work Packages to this folder's `Backlog Done <yyyy-mm>.md` (one note per month, newest at the top), with the date, the commit and one line on what was verified and how.

**One task per session.** When a task is finished:

1. its gates pass and the work is committed as `AGENTS.md` describes;
2. its row moves to the month's done note;
3. the "Next session" section at the top of [[Status]] is **overwritten**: the date, what was just finished (commit), the next task (its WP number), which pages and files it needs, and what the next session must know that is not written anywhere else. History belongs in the done note, not there;
4. the agent reports to Daniel and suggests a fresh session.

**Unattended runs.** When a task's design is written in its row (files, acceptance, nothing left for Daniel to decide), the maintainer's session may queue it for Codex or Mistral: one line `ready  WP-41  codex` (or `mistral`, `glm`) in the untracked `QUEUE.md` of the main checkout, then `dispatch-ready moonkale` (`--dry-run` first; `--max N`, `--hours H`). The script runs the queued tasks one after another, each on a fresh branch in the agent's worktree, and leaves `HANDOFF-from-<agent>-<id>.md` and a summary in `HANDOFF-from-dispatch.md` in the main checkout. It never merges or edits Status or the backlog: the next Opus session reviews each branch (and has the other model review it), merges, and closes the row as above.
