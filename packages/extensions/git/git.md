---
title: "ext-git — implementation notes"
tags: [crate-notes, milestone-7]
---
Notes for `moonkale-ext-git` (Milestone 7). Design: [[Version Management]], [[ADR-0012 Two histories]] (git stays authoritative for text).

- **Where git runs**: `WorkspaceConfig::git: Option<GitRun>` — `fn(root, GitRequest) -> Future<GitResponse>`. Desktop passes `cli::run` directly; web passes a server function (`api::git_run`, jailed to `MOONKALE_ROOT`). The panel never knows which.
- `cli.rs` (feature `cli`, not on wasm32): `git -C <root> …` with `GIT_TERMINAL_PROMPT=0`; `Status` = `status --porcelain=v2 --branch -z --untracked-files=all`; `Diff{path, staged}` (`--cached` for staged; untracked files via `diff --no-index /dev/null <path>`, whose exit code 1 is the normal case); `Stage` = `add -A --`, `Unstage` = `restore --staged --`, `Discard` = `restore --worktree` or `clean -f` for untracked; `Commit`; `Log{limit}` = `log --format=<RS/US separated> --name-only`; `Branches`, `Checkout`. Paths are checked (`..`, absolute) before they reach git. Not-a-repository → `GitResponse::Unavailable`.
- Types and parsers live in `ext-api::git` (`StatusEntry`, `Commit`, `parse_status`, `parse_log`) so the server function shares them.
- `panel.rs`: `GitState` (root-scope signals: status, log, open diffs, busy, epoch); `refresh` publishes `Workspace::vcs_status` (path → `(index, worktree)` letters) for the Explorer and tabs; the panel refreshes on its own epoch, `fs_epoch`, `graph_epoch` (saves) and source changes. `ChangesPanel`: branch/ahead/behind, commit box (`Ctrl+Enter`), Staged / Changes groups with per-row `+` `−` `✕`, log. `DiffPanel` (`git-diff:<s|w>:<path>` panels in the main tile, closable): coloured unified diff, *Open file*.
- `history.rs` — `GitHistorySource` (`SourceFamily::Custom("git")`): commit nodes (`NodeKind::Custom("commit")`, key `commit:<hash>`), file nodes (key = path), edges `parent` and `touches`. Added with `Workspace::add_source` by *Graph* / `git.history`; the Graph panel treats it like a trace (auto-picked, files open by path).
- Commands: `git.refresh`, `git.commit` (`Ctrl+Shift+G`, focuses the message), `git.history`.

Not yet: push/pull/fetch, branch creation, per-hunk staging, merge-view diffs, blame. Tests: `cli` round trip on a temp repository; E2E `git.mjs` (the fixture folder is a repository since `fixture.sh` of this milestone).
