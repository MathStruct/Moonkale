---
title: "moonkale and moonkale-server — which is which"
description: "The app you click and the server you host: what each one is, what each contains, when you need the second, and how they talk to each other."
tags: [packaging, server, start]
---
Every Moonkale package contains **two programs**, and the container image on GitHub contains only the second. They are built from the same crates and speak the same protocol; the difference is where the window is.

| | `moonkale` | `moonkale-server` |
|---|---|---|
| what it is | the **desktop app**: a window with the workbench in it | a **headless HTTP server** that holds a folder, plus the browser client it hands out |
| where the UI lives | in the window (WebKitGTK on Linux) | in your browser, or in another Moonkale that connects to it |
| where the folder lives | on the same machine, unless you connect somewhere | on the machine the server runs on |
| needs a graphical session | yes | **no** — a VPS, a Raspberry Pi, a container, a machine reached only by SSH |
| who starts it | you, from the menu or the shell | you (`moonkale-server`), systemd/Docker, or Moonkale itself over SSH |
| size | ~240 MB + 7 MB of editor assets | ~240 MB + ~52 MB browser client |

## What each one carries
Both binaries include the whole engine — sources (folders, SQLite, DuckDB, LadybugDB), the index, git, language servers, terminals, the Typst compiler, the wasm extension host and the agent. That is why both are large and why the numbers barely differ. `moonkale` adds the window and the editor bundles (`lib/Moonkale/assets`); `moonkale-server` adds the compiled browser client (`lib/Moonkale/public`, the wasm build of the same UI) and the HTTP surface: the server functions under `/api/…`, `/login`, the MCP endpoint at `/mcp`.

## When you want the server
1. **Your files are on another machine.** Run `moonkale-server` there, open a browser — the code-server model. Terminal, git, language servers and agent sessions all run on that machine; nothing is copied to the client.
2. **You want the desktop app but the files are elsewhere.** *File → Connect to Server…* in `moonkale` makes the app a client of a running server (sources, terminal, git, agent sessions there; the editor and your API keys stay here). The Android app does the same — that is the only way it opens a folder that is not its own.
3. **You want an agent that keeps working after you close the window.** `agent.on_server` runs turns on the server; the transcript is on its disk ([[Agent Sessions and Profiles]]).
4. **You do not run a server at all.** *File → Open Remote Folder…* still works: `moonkale` uploads `moonkale-server` to the host over your system `ssh`, starts it on that machine's loopback for the session, and shuts it down when you close the folder ([[Remote and Server Modes]]). You never type a server command.

## Running the server
```sh
moonkale-server --root /srv/notes --port 8080                  # loopback, no token: development
MOONKALE_TOKEN=$(openssl rand -hex 16) \
  moonkale-server --root /srv/notes --bind 0.0.0.0 --port 8443 \
  --token-stdin < token.txt                                    # or the flag, reading stdin
```
Flags: `--root` (the folder it serves, also `MOONKALE_ROOT`), `--bind`, `--port`, `--token-stdin`, `--version`. **A non-loopback bind is refused without a token, and without TLS unless `MOONKALE_INSECURE_HTTP=1`** says a reverse proxy terminates it; `MOONKALE_TLS_CERT`/`MOONKALE_TLS_KEY` make the server itself speak HTTPS. `MOONKALE_TERMINAL=0` switches the terminal off for an exposed server. Details and the threat model: [[Remote and Server Modes]].

In a container, all of that is the same:
```sh
docker run --rm -p 8080:8080 -v /srv/notes:/data \
  -e MOONKALE_TOKEN=… -e MOONKALE_INSECURE_HTTP=1 ghcr.io/mathstruct/moonkale-server
```

## Where the pieces sit on disk
```
bin/moonkale                  the desktop app
bin/moonkale-server           the server
lib/Moonkale/assets/          the desktop app's editor bundles (CodeMirror, Milkdown, xterm, KaTeX)
lib/Moonkale/public/          the server's browser client (index.html + the wasm build)
```
Both binaries look for their data **next to themselves**: `moonkale` for `../lib/Moonkale/assets`, `moonkale-server` for `public/` or `../lib/Moonkale/public` (P-126). Moving one binary out of a package without its directory gives you an app with no editors, or a server that serves the API and no client — it says so on startup rather than crashing.

Related: [[Install]] · [[Packaging Overview]] · [[Remote and Server Modes]] · [[Getting Started]].
