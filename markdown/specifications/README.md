---
title: "Specifications — small requests and bugs"
tags: [spec, moc]
---
Small, numbered notes (`001.md`, `002.md`, …) on how a UI element should behave, or a little bug — not big specifications (those are the milestone plans under [[Milestone 6 - Scale and Extend|milestones]]). Daniel writes them; whoever fixes one edits the same file: add `status: done` and a short "Done <date>" paragraph naming the change and the test that covers it. Bigger findings still go to the [[Problem Log]].

| # | topic | status |
|---|---|---|
| [[001]] | Flow editor: Backspace/Delete in a parameter field deleted the block | done |
| [[002]] | HelixDB as an embedded graph source — exists, but only as a git dependency for now | open |
| [[003]] | Projects: several sources at once, a selector, import/export and sync — desired behaviour in [[Projects and Sources]] | documented |
| [[004]] | Unicode input: `\int` → ∫ with a dropdown, in every text field — design in [[Unicode Input]] | documented |
| [[005]] | A Claude Code extension without an API key — plan in [[Claude Code Extension]] | planned |
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
