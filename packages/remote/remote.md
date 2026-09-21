---
title: "remote — implementation notes"
tags: [crate-notes, milestone-11]
---
Notes for `moonkale-remote` (Milestone 11). Desktop only. Design: [[Remote and Server Modes]], plan [[Milestone 11 - Remote]].

## What it does
`SshSession::open(SshTarget { host, path }, server_binary, on_phase)` turns a folder on another machine into a Moonkale server the desktop talks to:

1. **One `ssh` under a PTY** (`PtyBackend::spawn_args`) is the whole session: `ssh -M -S <tmp>/cm-<port> -o ControlPersist=no -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 -L 127.0.0.1:<local>:127.0.0.1:<remote> <host> -- sh -c '<script>'`. Its output goes through a `TeeBackend` to a **terminal tab** (so passphrase / password / host-key prompts are visible and typed by the user) and to the session's own reader, which watches for the script's marker lines.
2. **The remote script** (`remote_script`, plain `sh`): `mkdir -p ~/.local/share/moonkale/server/<version>`; if `moonkale-server` is missing there it prints `MOONKALE_NEED_UPLOAD <uname -m>` and waits for a `.ready` flag; then `stty -echo`, prints `MOONKALE_TOKEN?`, `read TOKEN`, prints `MOONKALE_STARTING`, and `exec`s the server with `--port <remote> --bind 127.0.0.1 --root <path> --token-stdin`, the token piped in through `printf`.
3. **Upload** (`upload`): a second `ssh -S <socket> -o BatchMode=yes host -- sh -c 'cat > …tmp && chmod +x && mv && touch .ready'` over the **multiplexed** connection — no second authentication — streams the binary from disk (`tokio::io::copy`, never read into memory: the debug server is 1.7 GB, the release one 188 MB / 142 MB stripped).
4. **Token**: `api::client::session_token()` (32 random bytes, hex) is written to the PTY when the script asks — with echo already off, so it never shows in the terminal tab — and becomes the bearer.
5. **Ready**: `wait_for_server` polls `POST /api/sources/list` on the forwarded port with the bearer (5 s request timeout, 60 s budget, fails fast if `ssh` exits) and then `api::client::connect(url, token, "host:path")`; from here the desktop's `WorkspaceConfig` callbacks dispatch to the server (`api::client::active()`).
   `connect` does not change dioxus's server URL (a `OnceLock`, P-098): the desktop's `api::relay` pipes to the active remote.
6. **Close** (`close`, also on drop): kill the master `ssh` → forward and remote server die (the server is a child of the ssh session); `api::client::disconnect()`.

`Phase` (`Connecting → Prompt(line) | Uploading → Starting → Ready { url } | Failed(msg) | Closed`) is pushed through the `on_phase` callback from the handshake, which always runs on a thread of its own with a current-thread runtime (the desktop's runtime is not to be relied on), and the `Workspace` turns it into status-bar text (`ext-api`'s `remote.rs`).

`server_binary()`: `MOONKALE_SERVER_BINARY`, else `moonkale-server` next to the executable, else the dev build under `target/dx/web/{release,debug}/web/server` (cwd and two parents up).

## Decisions
- **ControlMaster after all.** The plan said no control master; the upload needs a second authenticated channel and a second prompt would be a worse experience. `-o ControlPersist=no` plus the socket in a per-process temp dir means nothing outlives the master, which Moonkale kills on close.
- **Prompt detection is heuristic** (`looks_like_prompt`: `password:`, `passphrase`, `(yes/no`, `verification code:`), only for the status bar — the terminal tab shows everything anyway.
- **No checksum yet**: the binary travels over the same authenticated, encrypted channel and lands through an atomic `mv`; a version directory per release avoids mixing. Arch mismatch is not checked either: the script reports `uname -m`, the desktop logs it; a wrong binary fails to exec and the tab shows why (deferred to the release process, [[Milestone 11 - Implementation Log]]).
- **The remote server needs a `public/` directory** next to its cwd (dioxus-server asserts it): the script creates an empty one; the remote server serves no web client, only the API.

## Tests
- `cargo test -p moonkale-remote` — prompt heuristics, quoting, ANSI stripping, the script's shape.
- `cargo test -p moonkale-remote --test shim -- --ignored --nocapture` — the **whole session against a fake `ssh`** on `PATH` (a shell script that runs the remote command locally under a scratch `HOME`, turns the `-L` forward into a direct bind, and takes the upload on stdin): `Uploading → Starting → Ready`, the binary lands in `~/.local/share/moonkale/server/0.1.0/`, `open_folder` + `fetch_text` through the session read `Hello.md`, `close()` disconnects. Needs `cd packages/web && dx build --platform server` (the release build is found first). 0.9 s with the release binary.
