---
title: "Getting started — Moonkale for VS Code and Obsidian users"
description: What Moonkale is, in the words of someone who already uses VS Code and Obsidian; the four ideas that make it different; a first session; what you can expect to work today and what not.
tags: [tutorial, start]
---
You use **VS Code** for code and **Obsidian** for notes. Moonkale is one app that does both — and something neither does: it treats everything you open as **one graph**. This page is for you; it stays away from the internals (those are behind [[Overview]] when you want them).

## The four ideas

**1. Folders and databases are the same kind of thing.** In VS Code you open a folder; in Obsidian you open a vault. In Moonkale you open *sources*: a folder, but also a SQLite/DuckDB file, a CSV directory, a graph database. Each shows up in the Explorer with its own colour and icon, and each contributes to the same graph. You can have several open at once — two repositories, two vaults you are merging — and see them together.

**2. The graph is the point, not a feature.** Obsidian's graph shows notes and links. Moonkale's shows files *and* the functions, structs and modules inside them, *and* the wiki-links between notes, *and* the tables of a database — all connected. The Graph panel is a real renderer (GPU, 100 000 nodes), with a *Local* mode around the file you are in and a 3D mode with one plane per kind of thing. Click a node: it opens.

**3. Your vault stays your vault.** Notes are plain markdown with Obsidian's `[[wiki-links]]`, `[[note#heading|alias]]`, front matter and `$…$` / `$$…$$` KaTeX. Open the same folder in Obsidian tomorrow; nothing was changed behind your back. What Moonkale adds lives in one hidden folder, `.moonkale/`, that you can delete.

**4. Everything is an extension, and the agent is part of the app.** The editors, the terminal, git, the graph — each is an extension you can switch off in the Extensions panel (the puzzle icon). The **Agent** panel is a chat with an LLM that can *use* your sources: list them, read files, query the graph and the databases, search — and, only with your click on *Allow*, edit files. It works with your own keys (Anthropic, OpenAI-compatible, Ollama) or with **Claude Code on your subscription, no key** — a *Log in* button in Settings runs its browser sign-in. You can save several agents and run several sessions at once; their history stays in the folder.

## Coming from VS Code
| you are used to | in Moonkale |
|---|---|
| `Ctrl+Shift+P` palette, `Ctrl+P` quick open | the same keys, same idea |
| the activity bar on the left | the same: Files, Search, Links, Git, History, Agent, Terminal, Graph; Settings, Extensions and People at the bottom |
| the editor | **CodeMirror** by default (language servers, completion, rename, references, diagnostics) — you point Moonkale at the same `rust-analyzer`, `pyright`, `clangd`… on your `PATH`; a second, **Rust-built editor** with tree-sitter colours for Lean, Nix and Typst too, one click away (*Rust* in the toolbar) |
| the integrated terminal | `` Ctrl+` ``; on a remote folder the shell runs on the remote machine |
| Source Control | the **Git** entry: stage, commit, diffs, and the history *as a graph* |
| Remote-SSH | **File → Open Remote Folder…**: your system `ssh` (keys, config aliases, passwords typed in the terminal tab), the folder, its terminal, git and language servers run on the other machine, the editor here |
| Settings JSON | Settings panel with user and workspace scopes (`.moonkale/settings.json` in the folder); each extension's on/off switch and settings sit under the extension in the Extensions panel |
| extensions marketplace | none yet — built-in extensions, plus wasm modules you drop into a folder ([[Extension Catalogue]]) |

Differences you will notice: no debugger, no tasks, no multi-cursor in the Rust editor, and the language-server features exist in the CodeMirror editor only.

## Coming from Obsidian
| you are used to | in Moonkale |
|---|---|
| the editor (live preview) | **Rich** mode (WYSIWYG, the default for `.md`) or **Source** mode (a code editor with `[[` completion); the buttons are above the note |
| `[[links]]`, backlinks, unresolved links | the same links; **Links** in the activity bar shows backlinks; unresolved targets are dashed and a click creates the note |
| the graph view | the **Graph** panel — with the code and the databases in it too, not only notes |
| properties / front matter | a *Properties* bar above a rich note; the YAML is edited there, the source is untouched |
| MathJax | KaTeX, inline and block, with your `.moonkale/katex.json` macros |
| community plugins | extensions (fewer, mostly built in); Obsidian's plugins keep working on the same vault in Obsidian |
| Sync / mobile | no sync of its own (use git, Syncthing, or Obsidian Sync on the same folder); an Android app exists and can connect to a Moonkale server running on your desktop |

Differences you will notice: no canvas, no daily notes, no templates plugin, fewer themes; tabs, Mermaid and TikZ in notes are on the list, not done ([[Markdown Diagrams and Math]]).

## A first session (ten minutes)
1. **Install** — [[Install]]. Start `moonkale`.
2. **File → Open Folder…** — pick a repository or a vault. The Explorer fills; the status bar says `index: N files · links · symbols` when the graph is ready.
3. Click **Graph** (bottom-left). *Whole* shows everything; *Local* shows two hops around the file you have open; **3D** for the layered view. Scroll to zoom, drag to pan, click a node to open it.
4. Open a `.md` note: you are in **Rich** mode. Type `[[` — completion lists the notes. Type `$\int_0^1$` — it renders. The **Properties** bar holds the front matter.
5. Open a `.rs`/`.py`/`.jl` file: CodeMirror with the language server if you have one installed (the status bar says what it found). Try *Rust* in the toolbar for the other editor.
6. **Agent** (right): ask "what does this folder contain?" — the mock provider answers offline; in Settings → Agents add an agent: Claude Code (subscription; *Log in* opens the browser) or a provider with a key stored as a *secret* (never in settings). *New* starts a second session while the first works.
7. **Terminal** (`` Ctrl+` ``): a shell in the folder. Ctrl+click a `path:line` in its output to open the file.
8. **Ctrl+Shift+F** searches the folder — keyword search always; semantic search when an embedding model is configured.

## What to expect
Moonkale is at version 0.1: usable daily by its author, rough at the edges. Things that are solid: opening folders and databases, the graph, the rich editor with wiki-links and KaTeX, CodeMirror with language servers, git, history, the agent, remote folders over SSH. Things that are not there: a debugger, a marketplace, sync, most of Obsidian's plugin ecosystem, signed Windows/macOS builds. When something is wrong, say so — the [[Problem Log]] is where every reported problem ends up with its fix.

Next: [[Install]] · the [[Extension Catalogue]] (what each extension does and where it runs) · [[Remote and Server Modes]] (working on another machine) · [[Overview]] (how it is built).
