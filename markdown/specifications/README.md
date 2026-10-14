---
title: "Specifications — small requests and bugs"
tags: [spec, moc]
---
Small, numbered notes (`001.md`, `002.md`, …) on how a UI element should behave, or a little bug — not big specifications (those are the milestone plans — see the [[Roadmap]]). Daniel writes them; whoever fixes one edits the same file: add `status: done` and a short "Done <date>" paragraph naming the change and the test that covers it. Bigger findings still go to the [[Problem Log]].

| # | topic | status |
|---|---|---|
| [[001]] | Flow editor: Backspace/Delete in a parameter field deleted the block | done |
| [[002]] | HelixDB as an embedded graph source — done in Milestone 17 (git dependency, read-only; HTTP mode open) | done |
| [[003]] | Projects: several sources at once, a selector, import/export and sync — desired behaviour in [[Projects and Sources]] | documented |
| [[004]] | Unicode input: `\int` → ∫ with a dropdown, in every text field — design in [[Unicode Input]] | documented |
| [[005]] | A Claude Code extension without an API key — plan in [[Claude Code Extension]] | partly done — the provider (Level 3) since Milestone 12, login since 15; the IDE bridge (Level 2) open |
| [[006]] | Android graph: pinch zooms, two fingers pan, rotation orbits in 3D | done |
| [[007]] | Android app name + icon via `packages/mobile/build-android.sh` (dx 0.7.10 ignores both) | done |
| [[008]] | View png/jpg/svg inside Moonkale — image viewer extension (zoom, pan, SVG source) | done |
| [[009]] | Activity bar with badges, Ctrl+B/J, menus from the registry (Show ▸, Git/Agent, editor actions), source icons/colours/lock, phone More sheet | done |
| [[010]] | Syntax highlighting for the core languages (Lezer/legacy grammars in the view, P-093) | done |
| [[011]] | Every panel closeable (×), reopen via palette/rail/phone bar; empty tiles collapse and come back at their edge | done |
| [[012]] | `[[wiki-links]]` like in Obsidian: rendered, clickable, `[[` completion, create-on-click, rename rewrites (embeds + hover preview deferred) | done |
| [[013]] | Formulas (KaTeX) in the rich editor, with per-folder macros | done |
| [[014]] | Word wrap in the code editor: `editor.wrap`, toolbar button, Alt+Z, Settings → Editor | done |
| [[015]] | Close an open folder: Explorer context menu, File menu, palette; refuses while unsaved; no reopen on next start | done |
| [[016]] | A `.jl` file stays at "Loading editor…" on desktop — the editor mounted from an effect (P-047 family); now `onmounted`, and mount failures are shown | done |
| [[017]] | Graph view reset on resize / focus change — `set_graph` is incremental now (positions, pins and camera survive reloads) | done |
| [[018]] | Splices instead of the whole document per keystroke (P-037): 3 MB file, 87 ms per key | done |
| [[019]] | Front matter as a Properties bar in the rich editor, body-only view, saved intact | done |
| [[020]] | Several folders in one graph: every index drawn, per-folder colours, picker narrows to one; cross-folder links wait for projects | done |
| [[021]] | Markdown opens in Rich mode by default (`editor.markdown_rich`); loading no longer marks the file dirty | done |
| [[022]] | Julia and Python symbols in the index/graph (tree-sitter); calls/references still not extracted for any language | done — the extractors did not run until 2026-10-02 (issue #16, fixed in [[Milestone 18 - Implementation Log\|Milestone 18]] phase 1) |
| [[023]] | Why a pull costs a 30-minute build: two debug builds of 1.8 GB, feature changes, a thrashed `target/`; `debug = "line-tables-only"` applied: clean debug build 399 s, binaries 449 + 470 MB instead of 1.8 + 1.7 GB | done |
| [[024]] | Go to definition / centre a graph view from the symbol under the cursor (menu, double or right click); builds on `cursor_word` and the index's symbols | documented |
| [[025]] | A five-button mouse is the expected device: thumb buttons = back/forward, middle click = open aside, configurable; guard the webview's own button handling | documented |
| [[026]] | Rich editor typography: font size, body font, code font — settings under the Markdown extension | done |
| [[027]] | macOS keybindings: `Ctrl` in a binding means Cmd on a Mac and only Cmd; Ctrl stays the text system's (Emacs keys); `keys::primary` everywhere | done (unverified on a Mac) |
| [[028]] | Export/import the phone's vault (and open a real folder through the Storage Access Framework): private storage is lost on every uninstall and `run-as` only works on debug builds | open |
| [[029]] | The Android app's name and launcher icon come from a post-build patch that Gradle's caches can defeat (an APK said "Mobile" with the template icon); verify the artefact, then move it into configuration | partly done |
| [[030]] | The UI in English, German and Chinese; themes (dark, light, follow system, user/extension themes) | open — after Milestone 18 phase 4 (per-extension strings and settings) |

## Planned — not yet numbered

Requests that are agreed but have no spec number yet. Daniel numbers them when he writes the spec; until then they live here.

- **Components on the canvas** (2026-10-14). Small Dioxus components drawn over the graph and flow renderer, for example a table node in a schema editor (Supabase-like), a node's settings edited in place, or an Obsidian-like canvas of documents. They show only when zoomed in far enough, and only a few at a time. The shape:
  - a DOM layer above the GPU canvas and the label overlay, moved by one CSS transform per camera change, with components placed in canvas coordinates;
  - a node marked as embedded keeps its rectangle in the engine (wires, ports, drag, layout); only its label and symbol give way to the component;
  - below a screen size or off-screen, the component unmounts and the plain rectangle remains;
  - clicks pass through the layer except on components, and a card's header drags its node;
  - 2D only.

  What's missing:
  1. a camera-change event from the engine, which today reports only hover, click, double-click and settled;
  2. the embed flag;
  3. one shared layer component for the graph view, the flow editor and a canvas view;
  4. a contribution point for an extension to supply a node kind's view.

  The flow editor's parameter editor already floats over the canvas from the engine's screen reports, and is the first user.
- **Code editor at 60 FPS** (2026-10-14): requirement CED-R22 in [[Code Editor Requirements]], the gate in [[Rust Code Editor Roadmap]]. p95 frame interval ≤ 16.7 ms while typing, scrolling and selecting, in an ordinary file and the 3 MB one, web and desktop. The current Rust editor does not meet it yet.
- **A stripped-down static reader** (2026-10-14, **way later**): a repository or a knowledge graph published as wiki-like pages, Quartz-like but with code navigation, Moonkale's graph and in-browser SQL. It sits beside the full web version, not in place of it. Quartz stays for the vault until then. Notes in [[Publishing Sources]] ("Revisited 2026-10-14").
- **Math in any delimiter, LaTeX or Typst** (2026-10-14): `$`, `$$`, `\(`, `\[` and their doubled-backslash forms all work. Each formula renders as LaTeX (KaTeX) or Typst math, whichever it is, and errors only when both fail. A command normalises a file or folder to one convention. Spec in [[Math Delimiters and Renderers]], WP-36.
- **Shared work in a session** (2026-10-14): several people and agents in one session (roles, agents with a sponsor, tasks, an approval inbox, attribution); concurrent edits on one graph, where the database guarantees each write and Moonkale adds awareness, conflicts, proposals and per-actor undo; comments on anything, with `@agent` mentions. In [[Shared Work]], WP-48 to WP-50. Conversations already render Markdown.
- **Linked panels** (2026-10-14): a node's content and its context (the local graph, backlinks, rows, plots) form a link group that follows one subject. Each group has a tab colour plus a marker (never colour alone), and several groups can exist side by side, each with its own local graph. Plan in [[Linked Panels]], WP-59.
