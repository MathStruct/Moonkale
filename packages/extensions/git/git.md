---
title: "ext-git — implementation notes"
tags: [crate-notes, milestone-7]
---
Notes for `moonkale-ext-git` (Milestone 7). Design: [[Version Management]], [[ADR-0012 Two histories]] (git stays authoritative for text).

- **Where git runs**: `WorkspaceConfig::services` / `GitRunner` — `fn(root, GitRequest) -> Future<GitResponse>`. Desktop passes `cli::run` directly; web passes a server function (`moonkale_ext_git::git_run`, jailed to `MOONKALE_ROOT`). The panel never knows which.
- `cli.rs` (feature `cli`, not on wasm32): `git -C <root> …` with `GIT_TERMINAL_PROMPT=0`; `Status` = `status --porcelain=v2 --branch -z --untracked-files=all`; `Diff{path, staged}` (`--cached` for staged; untracked files via `diff --no-index /dev/null <path>`, whose exit code 1 is the normal case); `Stage` = `add -A --`, `Unstage` = `restore --staged --`, `Discard` = `restore --worktree` or `clean -f` for untracked; `Commit`; `Log{limit}` = `log --format=<RS/US separated> --name-only`; `Branches`, `Checkout`. Paths are checked (`..`, absolute) before they reach git. Not-a-repository → `GitResponse::Unavailable`.
- Types and parsers live in this crate’s `types` (`StatusEntry`, `Commit`, `parse_status`, `parse_log`) so the server function shares them.
- `panel.rs`: `GitState` (root-scope signals: status, log, open diffs, busy, epoch); `refresh` publishes `Workspace::contrib.file_marks` (path → a `FileMark` from `(index, worktree)` letters) for the Explorer and tabs; the panel refreshes on its own epoch, `fs_epoch`, `graph_epoch` (saves) and source changes. `ChangesPanel`: branch/ahead/behind, commit box (`Ctrl+Enter`), Staged / Changes groups with per-row `+` `−` `✕`, log. `DiffPanel` (`git-diff:<s|w>:<path>` panels in the main tile, closable): coloured unified diff, *Open file*.
- `history.rs` — `GitHistorySource` (`SourceFamily::Custom("git")`): commit nodes (`NodeKind::Custom("commit")`, key `commit:<hash>`), file nodes (key = path), edges `parent` and `touches`. Added with `Workspace::add_source` by *Graph* / `git.history`; the Graph panel treats it like a trace (auto-picked, files open by path).
- Commands: `git.refresh`, `git.commit` (`Ctrl+Shift+G`, focuses the message), `git.history`.

Not yet: general branch management, per-hunk staging, blame. Tests: `cli` round trip on a temp repository; E2E `git.mjs` (the fixture folder is a repository since `fixture.sh` of this milestone).

## Milestone 18 phase 4
- `types.rs` — the request/response types and the parsers, moved here from `ext-api/src/git.rs` (4.2b): `ext-api` knows no git.
- `GitRunner` — the platform's git as a **service** (`WorkspaceConfig::services`, `ws.service::<GitRunner>()`); the desktop provides the local `cli` runner (or `remote` while connected to a server), the web and the phone `remote`.
- `server.rs` — the server half (feature `server`, 4.3): `git_run` (`POST /api/git`) runs the CLI inside the jail (`moonkale_server_host::jail_dir`); `remote` is the client-side `GitRunner` over it. Was `moonkale_server::git_run`.


WP-64 (2026-10-28): Refresh and history command buttons expose current keybindings through command_hint and data-shortcut. Commit-staged remains a distinct parameterized operation from git.commit, which focuses the commit message field.


WP-63 (2026-10-28): panel actions use `icons::ToolbarActions` for the Simple/Detailed density and the extension’s user-selected chrome. Secondary controls stay reachable through a flat ⋯ disclosure. Terminal session tabs and document/query content remain navigation/content. See [[ADR-0027 Simple by default]].


## WP-51 — sync and conflicts (2026-10-29)

`sync.rs` owns remote selection, preconditions and conflict I/O; `cli.rs`
serializes requests per Git directory. The client supplies only a typed action,
never a URL, credential, command or option. A plan shows branch, named remote,
ahead/behind, dirtiness and an explicit lease commit. Execution rechecks the
whole plan, including repository identity and a fingerprint of the configured
fetch/push endpoints. Pull uses the repository's own merge/rebase/ff policy. Publish sets
the upstream; ordinary push never forces. A lease is bound to the reviewed
object ID rather than Git's mutable tracking-ref shorthand. Pull/Sync fetch before review so divergence is current; lease review retains
the previously fetched commit. A rejected push offers Pull.

Git inherits ssh-agent, credential helpers and system askpass. Terminal prompts
are disabled (there is no Moonkale credential field). Remote fetch/push URLs,
including rewrite expansion, are checked for embedded credentials before use;
unsafe URLs receive a credential-free refusal. Git remotes require HTTPS or
SSH off loopback; local file and loopback HTTP remotes are supported. No arguments or output are
logged. The web server runs under its own identity behind the host's existing
auth/origin gate. The jail also checks the repository root, preventing a folder
inside the jail from granting access to a repository above it.

Network stdout/stderr reach an in-memory progress buffer, polled every 250 ms
by the active client; the final output/error is retained verbatim in a text
view. Buffers are capped at 1 MiB and network commands at five minutes. They
are not persisted. Git's hooks and signing configuration remain in force;
continuation uses a noninteractive editor for the existing message.

`conflict_model.rs` parses bounded diff3 hunks (2 MiB, 4096 hunks), including
empty sides, CRLF and Unicode. `conflict::ThreeWayEditor` takes base/ours/theirs,
diff3 text and a result callback, independently of Git persistence and operation
controls, ready for SW-21's later adapter. Each hunk can select ours/base/theirs;
the whole result is editable. Existing partial resolutions and edits in the
working file are preserved on opening. Picks replace only their unique unresolved marker
and preserve edits elsewhere. During rebase the UI explains Git's reversed side
roles. Resolve rereads all three stages and the working file, refuses stale
content and unsafe paths, atomically replaces the file and stages it. Continue
requires no unmerged paths. Merge, rebase and cherry-pick support abort.
Text conflicts are bounded to 100 files and 16 MiB of aggregate text per refresh; binary/oversized files
require external resolution. Delete/modify conflicts offer an explicit keep-deletion action, distinct from
empty text. File-mode conflicts require external Git commands.

Sync is the single primary toolbar action when Git chrome is accepted; the
other actions stay in the shared overflow. Declined optional chrome retains
Sync there too. All sync/continue/abort operations are in the palette with
shortcuts; standard focused buttons and the result textarea support keyboard
and mouse. Strings are in English, German and Chinese.

Tests: CLI lifecycle tests use disposable local bare repositories (including
merge/rebase/cherry-pick resolution, abort, stale-save refusal, rejected push,
stale lease and publication). `git-sync.mjs` drives fetch/pull/push, rejection,
lease refusal, two-hunk merge and rebase resolution, abort and publication in
the production web UI; `git.mjs` retains the original local workflow. The
standalone fuzz target `git_conflicts` imports the production model directly.


Validation complete (2026-10-29): 12 Git/model tests and all AGENTS gates pass.
The reconciled browser matrix passes 50 cases: 25 smoke (including Git sync and
auth), four structural/native, two structural/CodeMirror, Sophia and 18 default
graph suites. A private Linux desktop resolved a rebase hunk, continued and
pushed to a local bare remote; screenshots were inspected. The parser's final
61-second fuzz run completed 890,427 executions without a crash; the standalone
fuzz library passes 26 regression tests. Dependency policy passes with the
existing cached advisory databases and dated exceptions. No new crate versions
were resolved: the portable progress timers and tempfile reuse locked packages.
See the root handoff and `target/next37-validation.json` for exact logs.

## Everyday operations (WP-52, 2026-10-29)

Hunk and single changed-line controls in each diff apply only the selected
server-produced rows to the index, in either direction. The server rereads and
compares the complete diff; no client patch reaches `git apply`. Deselected
removals become context and deselected additions disappear. CRLF and Git's
missing-final-newline markers survive. New and deleted text files support partial staging and unstaging too; renames
are exposed as literal path additions/deletions for selection. Symlinks,
submodules and mode-only changes use whole-file actions.

Branches, Undo/reflog, Stash and Blame live in the overflow and palette, with
shortcuts. Sync remains primary. Secondary operations use a typed review plan:
repository, HEAD, refs, status and tracked content are rechecked on confirmation.
Hard reset names affected files, commits leaving the branch, and lost edits.
A dirty switch refuses until the user selects stash-before-switch, which saves
tracked and untracked files. Failed switching leaves that stash available.
Branch deletion refuses unmerged branches; no forced deletion is exposed.
Stash pop retains its stash on conflicts. Cherry-pick and revert conflicts
reuse the existing three-way view and continue/abort controls. No operation
runs garbage collection, reflog expiry or pruning.

Commit operands are full object identities reachable through this repository's
refs or reflog, branch operands are verified local names, upstreams are existing
local/remote refs, and stash operands resolve by observed object identity to a
current selector. No endpoint or credentials enter these operations. P-237
records why typed stash commands omit global literal-pathspec mode.

`ContribState.gutter_marks` supplies inert line labels/hover text keyed by
NodeId. CodeMirror and the Rust editor render it through the same API; dirty
buffers clear stale annotations. Blame's previous-commit buttons walk the
reported historical file/commit; historical rows remain in the Git panel and
do not relabel the current editor with historical line positions.

`git-everyday.mjs` covers the production workflows and both editor gutters.
Real-CLI tests cover line/hunk staging and unstaging with LF, CRLF and no final
newline, stale diffs, separate hunks, dirty switching, untracked stash lifecycle,
reset review freshness, reflog recovery, branch lifecycle/upstream/comparison,
blame parent walking, revert and cherry-pick. Validation results belong in the
root NEXT-39 handoff; the section above describes implementation scope.

The pure patch-selection, blame and reflog parsers are in `everyday_model.rs`,
shared directly with the standalone `git_everyday` fuzz target. Selection uses
a bounded hash set of row numbers, avoiding quadratic membership checks.
CodeMirror shares one diff snapshot across all row callbacks; copies occur only
when a selected action is sent. Blame results are attached only if the same
clean document version remains active. Both views check that version later.
The native gutter uses its existing three-character presence slot (presence
wins when another cursor occupies that line), keeping hit-testing geometry
unchanged; full commit, author and subject remain available on hover.

History retention is explicit at the process boundary: commands disable
`gc.auto` and `maintenance.auto`, and fetch/pull refuse pruning even when the
repository config enables it. A real local-remote test deletes remote refs/tags
and verifies that the local recovery refs remain. Switching also passes
`--no-overwrite-ignore`; ignored obstructions are refused. Hard reset review
names target paths as well as current changes, so ignored obstructions are
included in the confirmed loss locations.

WP-52 validation complete (2026-10-29): 24 Git CLI/model tests, 28 standalone
parser regressions, all AGENTS gates and the reconciled 59-case browser feature
matrix pass. The final git_everyday fuzz run completes 1,141,732 inputs in
61 seconds without a crash. Both editor screenshots were inspected.

Linked worktrees take the same in-process lock, keyed by their common Git
directory, so branch refs and the shared stash stack cannot interleave within
this host. The CLI regression holds that lock and verifies another worktree
waits; external Git processes still have the documented race boundary.
