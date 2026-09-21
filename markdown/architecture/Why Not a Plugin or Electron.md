---
title: "Why not a VS Code / Obsidian plugin, and why not Electron?"
description: The two questions everyone asks first — answered with the reasons, the honest costs of the choice, and measured numbers for what Moonkale weighs in memory and on disk.
tags: [architecture, rationale, decisions]
---
Two questions from a friend of Daniel's (2026-09-21): *why not write a plugin for VS Code or Obsidian?* and *why do you dislike Electron so much that you use Dioxus?* — plus *how big is Moonkale in memory, actually?* This page is the long answer; the decisions it rests on are [[ADR-0001 Dioxus instead of Lumino and Tauri]], [[ADR-0002 Rust first, TypeScript behind traits]], [[ADR-0003 wgpu for graph rendering]] and [[ADR-0004 WASM components for extensions]]. The goal that makes the choice necessary is [[Julia and Lenticulum]].

## 1. Why not a plugin for VS Code or Obsidian?

Short version: **the thing Moonkale is built around — a graph over several kinds of sources at once — is not something either host has a slot for.** A plugin extends the host's model; it cannot replace it. In both hosts the plugin would end up as an iframe (a "webview panel") containing the whole application, talking to the host through message passing, with the host's model (files in a workspace; markdown files in a vault) in the way rather than helping.

### What Moonkale needs that a host does not offer

| need | VS Code | Obsidian |
|---|---|---|
| **Sources of several kinds as peers** — a folder, a SQLite/DuckDB database, a graph database, a derived index, later a Julia depot, all as nodes and edges of one graph ([[Graph-Native Model]], [[Data Sources]]) | the model is *files in a workspace*; a database is at best a virtual file system provider, and the built-in editors, search and git only see files | the model is *markdown files in a vault*; everything else is a plugin's private data |
| **A graph view that stays fluid at 100k nodes** (wgpu, Barnes–Hut, 2D/3D — [[Graph Rendering Options]]) | possible inside a webview panel (WebGPU works there), but the data has to cross the extension-host ↔ webview boundary as JSON on every change | Obsidian's graph is a canvas over the vault's links; it cannot be replaced, only shadowed by a second graph view |
| **Extensions in Rust/wasm with a Rust trait API** ([[Extension System]]) — a plugin *of* a plugin is not a thing | the extension API is TypeScript in the extension host; a Moonkale plugin cannot host third-party extensions of its own | same, with a smaller API and no process isolation |
| **The agent's tools are the sources** ([[LLM and RAG]]): traversal, index queries, patches through the same `Source` trait, with an audit trail and permissions per extension | AI features are Copilot's; a plugin gets chat participants and language-model access, not a tool surface over its own data model | none built in; plugins bring their own |
| **Desktop, browser and phone from one codebase**, plus a server mode ([[Platform Matrix]], [[Remote and Server Modes]]) | no phone; the browser version (vscode.dev) limits extensions to the web host | mobile exists but plugin capabilities are reduced; no browser |
| **Rich text + code + tables + terminal docked side by side** as first-class editors of the same document model ([[ADR-0008 Rust owns the document, JS is a view]]) | custom editors exist, but each is its own webview; the document model is VS Code's `TextDocument` | one editor (CodeMirror-based); custom views are possible, docking is fixed |
| **History as an entity log** (who/what/when across user, agent and git — [[ADR-0012 Two histories]]) | timeline API is per file; no cross-source log | none |
| **Licence and longevity** | the OSS core (Code – OSS) is MIT; the product builds are proprietary; the extension API is Microsoft's to change | proprietary; the plugin API is stable but the app is not ours |

The last row matters less than the first: the real reason is the first row. A tool whose centre is "one graph across a folder, a database and a wiki" cannot be a guest in a tool whose centre is "files".

### What the plugin route would have given — the honest cost of not taking it
- **VS Code's ecosystem**: thousands of extensions, every language's tooling, themes, keymaps, remote development, the debugger. Moonkale re-implements the *core* editing experience (CodeMirror + LSP client + git + terminal + palette + quick open + find/replace: Milestones 3 and 7) and will never match the long tail. Mitigations: LSP and DAP are protocols, so the language servers are the same ones; the [[Claude Code Extension]] bridge; and the plan is *not* to compete on breadth but to be the tool for the graph-shaped work.
- **Obsidian's ecosystem and sync**: community plugins, Obsidian Sync, mobile. Mitigation: the vault stays plain markdown with Obsidian's `[[wiki-links]]`, front matter and KaTeX (specs 010/012/019), so a vault is edited in both — this vault is.
- **Users already there.** A plugin ships to an installed base; a new app has to be installed. Nothing mitigates that except being worth it.
- **Effort**: months of work went into things a plugin gets for free (docking — [[ADR-0010 dioxus-workbench for layout]]; settings, keybindings, palette; the Android shell).

A halfway option was considered and dropped: a VS Code extension providing only the sources + graph view, and an Obsidian plugin providing only the graph. Each would be a second copy of the core in TypeScript, and neither host lets the graph drive the editor.

## 2. Why not Electron — and why Dioxus?

First a correction to the premise: **Moonkale's desktop app is a webview app too.** Dioxus desktop renders the UI in the *system* webview (WebKitGTK on Linux, WebView2 on Windows, WKWebView on macOS), and CodeMirror, Milkdown and xterm are still JavaScript in that webview ([[JavaScript Inventory]]). The objection is not to HTML rendering. It is to the **Electron stack**:

| Electron | Dioxus (as used here) |
|---|---|
| ships its own Chromium + Node per app: 150–250 MB on disk before a line of app code, a second browser on every machine, updated on the app's schedule (security patches included) | uses the webview the OS already has and updates; the app is one binary |
| JavaScript/TypeScript is the application language; the data model, indexing, parsing and I/O run under a garbage collector on the renderer's or Node's main thread unless moved to workers with message passing | **Rust** is the application language: rope, index, graph layout, sources, the wasm extension host, git, Typst run in the app's own threads (tokio), no GC, and the same crates run on the server and in the browser (wasm) |
| two processes (main + renderer) with IPC by design; the model is "a browser tab plus a Node backend" | one process on desktop; the same code as the web client, whose "backend" is the server — one architecture for three targets ([[ADR-0005 Server functions as the remote backend]]) |
| npm supply chain for the whole app (thousands of transitive packages in a typical Electron app) | crates.io for the app (991 crates in the lock file — not small either), npm only for the three view bundles, each one dependency, pinned, built into one file ([[JS Interop Boundary]]) |
| GPU work goes through Chromium's compositor and WebGPU | the graph renderer is wgpu: native Vulkan/Metal/DX12 when it runs natively ([[ADR-0011 Desktop graph surface strategy]]), WebGPU/WebGL2 inside a webview or a browser |
| no phone; web needs a second build | desktop, web and Android from one crate tree ([[Platform Matrix]]); the Android APK is 23 MB |
| the extension model is JS in the same process | extensions are wasm modules with an explicit permission list ([[ADR-0004 WASM components for extensions]]), run by wasmtime natively and by a Worker in the browser |

Tauri would have kept the webview idea but with a TypeScript front end and a Rust back end joined by IPC — the original plan ([[old/rough goal]]) — and [[ADR-0001 Dioxus instead of Lumino and Tauri]] records why one language for both halves won: the extension API can be a Rust trait, the document model is shared with the server, mobile comes with it.

### The honest costs of Dioxus
- **Youth.** Dioxus 0.7 changed every API; the bugs in the [[Problem Log]] with a Dioxus name on them (P-047 evals from hooks lost on desktop, P-077 `serde(rename_all)` on enum variant fields, P-094 `onload` never fires, P-096 the server asserting a `public/` directory) each cost hours. `dioxus-workbench` exists because nothing like Lumino did.
- **Three webviews, three behaviours.** WebKitGTK lags Chromium (no WebGPU on Android's WebView either — P-089 — hence the WebGL2 fallback); WebView2 and WKWebView are untested here ([[Platform Matrix]]).
- **JavaScript is still in the loop** for the three editors; the JS-free desktop needs a Rust-native code editor and rich-text editor ([[Rust-native Editor Candidates]]), which is real work not yet done.
- **Compile times and binary size.** A clean release build of the server is ~4 minutes (DuckDB alone ~3, measured in [[Milestone 9 - Implementation Log]]), the desktop release 6 minutes; debug binaries are gigabytes because of debug info. Electron apps pay this in a different currency (download size, startup), but they do not make the developer wait.
- **A smaller ecosystem.** No React component libraries, fewer ready-made widgets; the table grid, the flow canvas and the docking were written here.

## 3. How big is Moonkale, actually?

Measured 2026-09-21 on the Linux dev machine (x86_64, release profile, `strip` not set for the native profiles).

### In memory
The **server process** is the same core the desktop runs in-process — sources, index, DuckDB, wasmtime, git, Typst, LSP client — without any UI:

| state | RSS |
|---|---|
| `moonkale-server` idle, nothing open | **22 MB** |
| the E2E fixture folder opened and indexed (a dozen files) | 28 MB |
| the Moonkale repository opened and indexed — 750 files, 785 links, 2 054 symbols | **51 MB** |

The **desktop app** adds the system webview on top of that. It could not be measured from this shell (no display — P-097), so the number for the app as a whole is Daniel's to take:

```sh
ps -o rss=,comm= -C moonkale,WebKitWebProcess,WebKitNetworkProcess | awk '{s+=$1; print} END {printf "total %.0f MB\n", s/1024}'
```

Expect the WebKit web process to dominate: a page with CodeMirror, Milkdown, xterm and the wasm graph renderer loaded is typically 100–250 MB in WebKitGTK, so **~150–300 MB for the whole app with a folder open** is the reasonable expectation, against the 22–51 MB of the Rust side. For comparison, publicly reported idle figures for the Electron editors are in the 300–500 MB range for VS Code (spread over five or six processes) and 200–300 MB for Obsidian; treat both as ballpark, not measurements made here. The graph renderer's layout numbers at 100k nodes are in [[Milestone 6 - Implementation Log]].

### On disk
| artefact | size | why |
|---|---|---|
| `moonkale-server` (release) | 188 MB, **142 MB stripped** | DuckDB (bundled), wasmtime, tree-sitter grammars, Typst + its fonts, rustls, tokio, the whole `api` |
| `moonkale-server` (debug) | 1.7 GB | debug info; never ship or upload this one |
| desktop app (release, Linux) | 193 MB, **147 MB stripped**, + 6.9 MB of assets (the three JS bundles, the renderer wasm, KaTeX, fonts); the debug build is 1.8 GB; a clean release build takes 6 minutes | the server's crates plus the UI, minus the axum side — the two binaries are nearly the same size because the batteries are the same |
| Android APK (release, arm64) | 23 MB (39 MB when stale assets pile up — P-092) | no DuckDB, no wasmtime, no tree-sitter on the phone |
| web client assets | Milkdown 2.7 MB, graph renderer wasm 2.4 MB, CodeMirror 1.0 MB (with the Lezer grammars, P-093), xterm 340 KB, plus the app's own wasm | the three JS bundles are the [[JavaScript Inventory]]; the renderer is a separate wasm module |
| source | 34 k lines of Rust across 40 crates, 1 k lines of TypeScript in three view packages | |

Two things follow. **The binary is big because the batteries are inside** — an Electron app with the same features would carry DuckDB, a wasm runtime and a Typst compiler as native node modules or separate downloads, on top of its Chromium. And **memory is where the choice pays**: the whole data side of Moonkale, with a real repository indexed, fits in the space Electron uses for an empty window.
