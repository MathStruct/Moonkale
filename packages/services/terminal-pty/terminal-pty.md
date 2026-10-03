---
title: "terminal-pty — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-terminal-pty` (Milestone 3). Native only.

`PtyBackend::spawn(cwd, cols, rows)` opens a pseudo-terminal with `portable-pty` 0.9, runs the user's shell (`$SHELL`, else `/bin/sh`; `cmd.exe` on Windows — untested) and returns a `TerminalBackend`. A std thread reads the master side and pushes chunks into an unbounded channel; `write` goes straight to the master writer; `resize` calls `MasterPty::resize`. Dropping the backend kills the child.

Used in-process by the desktop app and on the server behind `api::terminal_socket` (cwd jailed to `MOONKALE_ROOT`).

Tests: `cargo test -p moonkale-terminal-pty` (`echo` round trip through a real PTY).

## Milestone 11
`spawn_args(cwd, program, args, cols, rows)` runs a program with arguments (`ssh …` for `moonkale-remote`); `spawn_with_env` adds environment variables for the child (`SSH_AUTH_SOCK=0`); `spawn` delegates to them. `kill()` ends the child now (a remote session being closed) without waiting for the drop. `is_running()` lets the session fail fast when `ssh` exits.
