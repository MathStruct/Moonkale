---
title: "Projects and Sources — desired behaviour"
description: A project is a saved Moonkale state — a named set of sources (folders, remote folders, git-host repositories, databases, APIs), each openable and closable on its own, some read-only, each with its own colour — with a selector, import/export and synchronisation, and honest failure notices when a source is gone.
tags: [architecture, sources, projects, design]
---
Requested in [[Prompt9]] (2026-09-19): opening several folders at once turned out to be good design, not a bug — so make it deliberate. This page is the **desired behaviour**, written before any code; the implementation plan will be a milestone note that references it. What exists today is marked *(exists)*; everything else is new.

## Two words

- A **source** is anything Moonkale can browse, query and (maybe) edit through the `Source` trait *(exists)*: a folder, a folder on another machine, a repository on a git host, a database, an API. A source has a `SourceDescriptor { id, display_name, family, capabilities, root }` *(exists)*; `capabilities.write` says whether it can be written at all *(exists)*.
- A **project** is a saved, named, persistent piece of Moonkale state: which sources belong together, how each is configured, which documents were open and how the panels were laid out. **A project is not a folder** and not bound to one. It lives in Moonkale's own storage, not inside any source. Folders are members of projects, never the other way round.

Today's implicit behaviour — `recent_folders`, "reopen the last folder", `.moonkale/settings.json` per folder *(exists)* — becomes a project called **Default** that behaves exactly as now, so nothing changes for someone who never creates a project.

## Sources

| kind | what it is | platforms | write | status |
|---|---|---|---|---|
| **Folder** | a directory on this machine (desktop) or under `MOONKALE_ROOT` on the server (web) | all | yes | *(exists)*, several at once *(exists)* |
| **Remote folder** | a directory on another machine reached over SSH, the way Zed does it: Moonkale starts (or connects to) **its own server** on the remote host and uses it through `RemoteSource` — so LSP, terminal, git and the index run *there*, not over sftp | desktop, web (server → server) | yes | new; the transport is `ssh -L`/`ssh host moonkale-server` + the existing web protocol ([[ADR-0005 Server functions as the remote backend]]) |
| **Git-host repository** | GitHub / GitLab / Codeberg (Forgejo) by URL. Two levels: **browse** through the host's REST API (tree, blobs, commits, issues and PRs as nodes — read-only, no clone), or **check out** into a cache folder (`~/.cache/moonkale/repos/<host>/<owner>/<repo>`) which is then a Folder source with git, writable if the token allows push | all (API browse works from the browser too) | browse: no; checkout: if permitted | new; token is a `SecretRef` |
| **Database** | SQLite, DuckDB, LadybugDB files *(exist)*; Postgres, Turso, Redis, TypeDB, Helix by connection *(stubs)* | native + via server | per driver and per user choice | *(exists)* for files |
| **API** | information behind a REST/GraphQL endpoint — an OpenAPI/GraphQL description lifted into nodes (resources → `Table`-like nodes, items → rows) | all | rarely | new; realistically an **extension contribution** (`source` in the manifest, see [[Example - Data Source]]) with a generic OpenAPI one built in later |
| Index | derived over the others *(exists)* | all | no | *(exists)*, spans all open sources |

### Read-only is a property of the pair (source, project)
Three layers, any of which can say no:
1. the driver (`capabilities.write == false`: a DuckDB file, an API browse) *(exists)*;
2. the credentials (a GitHub token without push, a DB user without `INSERT`) — reported by the driver at connect time as `write: false` with a reason;
3. **the user**, per project: "treat this source as read-only here" (a production database you only want to look at; someone else's repository). Stored in the project, shown as a lock, enforced in the editor (no edits, Save disabled), in file operations, and in the **agent's policy** (writes refused before they reach the gate).

Explorer roots, tabs and the status bar show the lock; the reason is in the tooltip ("read-only: token has no push permission").

### Open and close individually
- Every source can be **opened** into the project and **closed** from it without touching the others: Explorer root context menu → *Close source*; commands `source.open.folder`, `source.open.remote`, `source.open.repo`, `source.open.database`, `source.open.api`, `source.close` (palette *(exists)*).
- Closing a source closes its documents (asking about unsaved ones), drops it from the index and the graph, and — this is the difference from today — **keeps it in the project as *suspended***: listed greyed in the Explorer, reopened with one click, not connected. *Remove from project* is a second, explicit action. A laptop away from the office keeps its database sources in the project without trying to connect.
- The **Index** always spans exactly the open sources; links across sources resolve (a note in one folder can `[[link]]` to a file in another, and the Links panel says which source the target is in).

### Colours
Each source in a project has an **accent colour**: chosen by the user or assigned deterministically from a palette (hash of the source id, so it is stable across machines). It appears as a stripe on the Explorer root, on the tab of every document from that source, in the editor's header line, as a tint on the source's nodes in the graph, and next to the file name in the status bar. Themes define the palette (eight colours that work on both dark and light backgrounds); the colour never carries meaning beyond "this source" — read-only is the lock, not a colour.

## Projects

### What a project stores
```jsonc
// ~/.config/moonkale/projects/<id>.json   (desktop; server: per user under MOONKALE_CONFIG_DIR; mobile: files/projects/)
{
  "version": 1,
  "id": "9f2c…",                       // stable uuid; the file name
  "name": "Moonkale",                  // shown in the selector
  "created": "2026-09-19T20:00:00Z",
  "updated": "2026-09-19T21:10:00Z",
  "origin": "daniel-laptop",           // machine that wrote `updated` (for sync)
  "sources": [
    { "kind": "folder",   "id": "src:folder:…", "path": "${HOME}/Code/Moonkale", "open": true,  "color": "#5b8def", "read_only": false },
    { "kind": "remote",   "id": "…", "host": "build-box", "path": "/srv/moonkale", "open": false, "color": "#e0a84a" },
    { "kind": "repo",     "id": "…", "url": "https://github.com/DioxusLabs/dioxus", "mode": "browse", "token": { "secret": "github-daniel" }, "read_only": true },
    { "kind": "database", "id": "…", "driver": "postgres", "params": { "host": "db.local", "db": "app", "user": "ro" }, "secret": "pg-app-ro", "open": true, "read_only": true }
  ],
  "layout": "…PanelLayout::encode()…",  // moves here from .moonkale/settings.json
  "open_documents": ["src:folder:…/packages/ui/src/frame.rs"],
  "active_document": "…",
  "settings": { "llm": { "provider": "mistral" } }   // project-scope overrides, same SettingsFile shape
}
```
- **Never a secret** in the file — `SecretRef` names only, resolved by the platform keychain/`secrets.json`/env *(exists)*. Importing a project on another machine therefore asks for the secrets it names, once.
- **Paths are portable** where they can be: `${HOME}` and named **roots** (`"roots": { "code": "/home/daniel/Code" }` per machine in the user settings, `"path": "${code}/Moonkale"` in the project). A project written on one machine opens on another after the roots are mapped; unmapped roots are reported, not guessed.
- **Settings scopes** become `user < project < folder`: the folder's `.moonkale/settings.json` *(exists)* keeps folder-bound things (git, extension permissions granted for that repo); layout and open documents move up to the project, because they describe the *set*, not one folder. The Default project keeps writing them to the folder so today's files stay valid.
- Window state (size, position) is not project state; presence rooms are per source, as now.

### The selector
- **Titlebar**: the project name is a dropdown left of the window title (`Moonkale — vault` today) listing the projects, most recently used first, with *New project…*, *Import…*, *Manage…*. Palette: `project.switch`, `project.new`, `project.rename`, `project.duplicate`, `project.delete`, `project.export`, `project.import`. Phone shell: the same list under the Files tile's header.
- **Switching** saves the current project (layout, documents, source state), closes its sources, opens the target's, restores its layout. Unsaved documents block the switch with the usual prompt. A window shows exactly one project; a second window may show another (the session channel *(exists)* keeps them apart by `WindowId`).
- **New project** starts empty, or *from the current sources* ("save these as a project"), or *from a folder* (the folder becomes its first source and the project takes its name).
- **Manage** is a Settings tab: rename, colour per source, read-only toggles, remove, the roots map, and the failures of the last open (below).

### Failures are reported, never hidden
Opening a project opens every source marked `open` and produces a **report**, one line per source:

| state | when | offered actions |
|---|---|---|
| `ok` | connected | — |
| `missing` | folder or file does not exist (moved, other machine, USB drive) | *Locate…* (rewrites the path, offers to add a root), *Suspend* (keep in the project, don't try), *Remove* |
| `unreachable` | SSH host, database or API not answering | *Retry*, *Suspend* |
| `unauthorised` | `SecretRef` not present on this machine, or rejected | *Enter secret…*, *Suspend* |
| `unsupported here` | a remote folder on the web client, a native driver on mobile | *Suspend* (it stays for the machines that can) |
| `read-only now` | connected, but with fewer rights than the project expects | none needed — the lock shows, tooltip says why |

The project opens with whatever succeeded; the report is a notice in the status bar with *Details* → the Manage tab, and it is written into the project file as `last_open` (so the sync target also knows). **Nothing is removed from a project by a failure** — only the user removes.

### Import, export and synchronisation
- **Export** writes the project file as `<name>.moonkale-project.json` (secrets stripped by construction, paths with `${roots}`); **import** reads one, assigns a new `id` if it collides, maps roots, asks for the secrets it names, and runs the open report. Drag a project file onto the window or `moonkale --project file.json` on the command line.
- **Sync**, three tiers, all using the same file:
  1. **A synced folder** — set `projects_dir` in user settings to a directory Syncthing/Nextcloud/git already carries. Zero infrastructure. Conflict rule: **last `updated` wins, per file**; the loser is kept as `<id>.conflict-<origin>.json` and the selector shows a *conflict* badge with a side-by-side (sources added/removed, layout differs).
  2. **The hub** — the same server that does presence *(exists)* stores projects per user (`/api/projects`, bearer token *(exists)*); the desktop pulls on start and pushes on save; the web client uses it as its only store. Same conflict rule, resolved on the client.
  3. **A git repository of projects** — tier 1 with history; nothing to build.
- What syncs and what does not: the project file syncs; **the sources' data does not** (a folder stays on its machine — that is what remote folders and repositories are for); secrets do not; per-machine roots do not; `last_open` reports carry `origin` so a stale "missing" from another machine is shown as such.

## How this maps onto what exists
- `Workspace.sources: Signal<Vec<SourceHandle>>` *(exists)* is already the open set; `open_folder`, `attach_source` *(exist)*; the Explorer already draws several roots. Missing: `close_source`, suspended entries, per-source `color`/`read_only`, the project file, the selector, the report, the remote/repo/API kinds.
- `SettingsFile` *(exists)* gains a project scope and the `roots` map; `recent_folders` becomes the Default project's source list.
- `RemoteSource` + the web server *(exist)* are the remote-folder backend; the new part is launching/tunnelling the server over SSH and treating that connection as a source rather than as "the" server.
- `Capabilities.write` *(exists)*; a `read_only` override is one field on the handle, checked where `Op`s are applied *(exists)* and in the agent policy *(exists)*.
- Presence rooms are per folder id *(exists)* — per source id after this, unchanged in spirit.

## Open questions (to decide in the implementation plan)
1. Does the Default project write layout to the folder forever, or migrate once a second project exists? (Leaning: write both while Default is the only project.)
2. Remote folders: start the remote server by SSH (`ssh host moonkale --server --stdio`) or require it to run already? Zed starts it and installs the binary if missing; that is the better experience and the bigger job.
3. Git-host browse: which objects become nodes beyond files — issues and PRs are useful for the graph and the agent, but every host API differs; start with the tree and commits.
4. One index across sources with different ids: the `IndexSource` already resolves by key; cross-source `[[links]]` need a precedence rule (same source first, then by project order).
5. Colour on the phone: a stripe on the tile header is all the room there is.

## Not in scope
Real-time merging of edits across machines (CRDT — [[Collaboration]]), syncing source *contents*, a hosted project store beyond the hub, per-project extension installation.
