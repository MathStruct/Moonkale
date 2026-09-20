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
| [[006]] | Android graph: no pinch-zoom or two-finger rotation | open |
| [[007]] | Android app is called "Mobile" and has no logo — the release build should have the right name and icon | open |
| [[008]] | View png/jpg/svg inside Moonkale — image viewer extension (zoom, pan, SVG source) | done |
| [[009]] | Activity bar with badges, Ctrl+B/J, menus from the registry (Show ▸, Git/Agent, editor actions), source icons/colours/lock, phone More sheet | done |
| [[010]] | Syntax highlighting for the core languages (Lezer/legacy grammars in the view, P-093) | done |
| [[011]] | Every panel closeable (×), reopen via palette/rail/phone bar; empty tiles collapse and come back at their edge | done |
| [[012]] | `[[wiki-links]]` like in Obsidian: rendered, clickable, `[[` completion, create-on-click, rename rewrites (embeds + hover preview deferred) | done |
| [[013]] | Formulas (KaTeX) in the rich editor, with per-folder macros | done |
| [[014]] | Word wrap in the code editor: `editor.wrap`, toolbar button, Alt+Z, Settings → Editor | done |
| [[015]] | Close an open folder: Explorer context menu, File menu, palette; refuses while unsaved; no reopen on next start | done |
