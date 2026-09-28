---
title: "Milestone 16 — Sources follow the disk: the plan"
description: The Explorer (now "Sources") follows changes on disk, with a refresh button for sources that are not watched; the graph's Local view gets its own layout and gives the Whole view back unchanged; Milkdown stops writing <br /> for empty lines and no longer hangs at "Loading rich editor…"; tabs can be dragged by touch. Record in Milestone 16 - Implementation Log.
tags: [milestone, planning, explorer, sources, graph, markdown, android]
---
From [[Prompt26]]. Record: [[Milestone 16 - Implementation Log]]. Background: [[Data Sources]], [[Graph View]], [[Markdown and Typst Editor]], [[P-045 Cross-window drag and drop]].

## What was asked, read carefully
1. **The Explorer does not follow changes** made outside Moonkale (another editor, `git checkout`, an agent in a terminal). Track them. Some sources need no tracking (a repository imported as a source and never edited): those get a **refresh button** instead.
2. Rename **"Files"** in the left bar to **"Sources"**: it holds folders, databases, later repositories.
3. **Graph → Local** spreads its few nodes oddly. Lay the neighbourhood out on its own, and keep the **Whole** view as it was when switching back.
4. Milkdown writes **`<br />`** into empty lines. It should not.
5. **Android**: tabs cannot be dragged to rearrange the panels.
6. **Linux**: "dragging between two windows might not follow the cursor but move faster sometimes".
7. **Linux**: the rich editor sometimes stays at **"Loading rich editor…"**.

## Why each happens (read in the code)
1. Nothing watches: `SourceEvent` is a stub (`core/src/source/event.rs`), `project-fs/src/watch.rs` a design note. The Explorer only reloads on `fs_epoch`, which only Moonkale's own file operations bump.
3. The Local graph is a subset of the Whole one, so the renderer's incremental `set_graph` (spec 017) keeps every node where the Whole layout put it: two hops of neighbours scattered across the whole map. Its own layout then moves those nodes, and Whole comes back disturbed.
4. Milkdown's CommonMark preset registers `remarkPreserveEmptyLinePlugin`; with it, the paragraph serializer writes every empty paragraph as an HTML node `<br />` so that blank lines survive a round trip.
5. The workbench's tabs use HTML5 drag and drop (`draggable`, `dragstart`, `drop`); touch never starts an HTML5 drag in the Android WebView.
6. Not found. The splitter's arithmetic (`start + Δpointer / span`) is scale-consistent, the display runs at scale 1, and nothing sets a CSS zoom. Which drag is meant (splitter, a tab between tiles, a tab between two OS windows) is the question to settle with Daniel before changing anything.
7. The mount script waits without limit at three points: the host element (`if (!el) return;` — silently), `window.moonkale.milkdown` (the bundle loaded through `document::Script`, a head element — P-087 showed head elements of the first render can go missing), and `crepe.create()`. Any of them leaves the panel at "Loading…" forever with nothing in the log.

## Scope
1. **Watching** — `Source::changes_since(seq) -> Option<Changes { seq, paths, reset }>`, a long poll (default `None`: not watched). `FolderSource` answers from a `notify` watcher started on first use; it watches only directories the Explorer would show (no hidden, nothing `.gitignore`d — `target/` alone has millions of files), adds directories as they appear, and keeps a bounded log of changed relative paths. `RemoteSource` forwards to a new server function `/api/sources/changes`, so the browser, the phone and remote SSH folders get the same. The Workspace runs one loop per source, resolves the paths (the `path` dialect), refreshes the index for them and bumps `graph_epoch` + `fs_epoch` — the Explorer and the graph already follow those. `Workspace::watched` tells which sources are followed.
2. **Refresh** — a ↻ button on every source that is not watched, and *Refresh* in each source's context menu; it refreshes the index from the root and reloads the tree.
3. **"Sources"** — the panel title and the activity label.
4. **Graph** — `GraphView::stash(name)` / `unstash(name)` (graph, positions, layout, camera) and a `fresh` flag on `set_graph`. Whole → Local stashes `whole` and loads the neighbourhood fresh; Local → Whole restores `whole` before the reload, which then only confirms it.
5. **`<br />`** — the wrapper drops lines that are only `<br />` and collapses runs of blank lines, outside fenced code, in everything it reports (changes, `getText`, the baseline after load). Old files with such lines open clean and lose them on the next edit (spec 021's rule).
6. **Loading** — every wait gets a limit and a message: the element (1 s), the bundle (the script inserts it itself if absent, 20 s), `crepe.create()` (20 s). A failure shows the reason and a **Retry** button instead of "Loading…", and is logged.
7. **Touch drag** — a small script installed with the workbench on touch devices: a long press on a tab starts a drag, the finger's position drives `dragenter`/`dragover`, lifting it drops, as synthetic drag events the workbench already handles.
8. Verify: unit tests (the watcher: create/modify/delete/new directory/ignored/hidden; the `<br />` cleaner via the bundle in a bare page), a `VirtualDom` test where it fits, E2E (`watch.mjs`: a file written behind Moonkale's back appears in the tree and the graph; a database source shows ↻; `graph.mjs` Local/Whole; `rich.mjs` no `<br />`; a touch drag under Chromium's touch emulation). Log, [[Problem Log]], Home/README.

## Decisions
- **Long poll, not a websocket.** One more server function on the channel every client already has (including the SSH relay); an idle folder costs one pending request per client, reconnection is a new request. A websocket per source would add a second transport to keep alive.
- **Paths, not events.** A watcher's create/modify/delete/rename distinctions are unreliable across platforms and editors (atomic saves are rename-over). The log says *which paths changed*; the Workspace asks the source what they are now. A burst (a `git checkout` of hundreds of files) becomes `reset`: refresh from the root.
- **Watch what the Explorer shows.** Hidden entries and `.gitignore`d ones are skipped by the watcher exactly as by `Children`; `.moonkale/` never triggers a refresh (Moonkale writes there itself).
- **Open documents are not reloaded yet.** An unmodified open file changed on disk would be the natural next step; it touches the document model and conflicts with the version check, so it is its own step (noted in the log).
- **The drag fix is a shim, not a fork.** `dioxus-workbench` is a crates.io dependency; synthetic drag events drive its existing handlers without patching it.
