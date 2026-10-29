---
title: "P-237 — Literal pathspecs break stash cleanup"
tags: [problem, git, testing]
status: resolved
phase: WP-52
---
**Status:** resolved in the contributor implementation · **Related:** [[Version Control Requirements]]

## Problem

The WP-52 real-CLI test found that Git 2.56.0 with global
`--literal-pathspecs stash push --include-untracked` returns success and saves
untracked files, yet leaves those files in the worktree. A branch switch could
then carry those files into the other branch. Plain `git stash` cleans them.

## Options

| | approach | consequence |
|---|---|---|
| A | Remove global literal mode everywhere | loses the existing file-path protection |
| B | Omit literal mode for typed stash commands only | preserves Git's internal pathspec handling; no client pathspec enters stash |

## Measurements

2026-10-29, Git 2.56.0, disposable repositories: both invocations save the
untracked file in the stash's third parent. With literal mode, porcelain
status still reports `? untracked`; without it, status is empty.

## Decision

B. `cli::git` keeps literal mode for file operations and omits it only when
the typed command starts with `stash`. Requests accept messages, booleans and
server-resolved stash identities; they cannot supply stash pathspecs or argv.
The regression covers stash-before-switch, apply, pop and drop with an
untracked file and checks both saved content and its removal/restoration.

## Lesson

Git's global options also reach the subprocesses of built-in workflows.
Check disk state as well as command success and stash contents.
