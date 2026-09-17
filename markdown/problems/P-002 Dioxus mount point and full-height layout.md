---
tags: [problem, ui]
status: resolved
phase: 0
---
# P-002 — Dioxus mount point and full-height layout

**Status:** resolved (2026-09-17)

## Problem
`dioxus-workbench` fills its parent; the dummy UI gave `body` a flex column and `#home { flex: 1 }`, but the workbench rendered ~400px tall. Dioxus mounts the app into `<div id="main">` inside `body`, which wasn't part of the flex chain.

## Resolution
`#main { flex: 1; min-height: 0; display: flex; flex-direction: column }` in each platform's `main.css`. Verified by headless-Firefox screenshot of the web build.

## Lesson
Any full-height layout must account for `#main`. Belongs in the shell's base stylesheet once `ui` owns global CSS.
