---
tags: [problem, ui, theme]
status: resolved
phase: 0
---
# P-003 — dioxus-workbench theme variables

**Status:** resolved (2026-09-17)

## Problem
Overriding `--wb-*` on `.wb-shell` didn't darken the workbench: the crate declares its variables on **both** `.wb-shell` and `.wb-workspace` (at zero specificity via `:where`), so the nested workspace re-applied the light defaults over the inherited override.

## Resolution
Target both: `.editor-root .wb-shell, .editor-root .wb-workspace { --wb-canvas: … }`. The app forces a dark body, so the workbench is pinned dark rather than following `prefers-color-scheme`.

## Lesson
`ThemeContribution` ([[Contribution Points]]) must set variables on both selectors. Consider upstreaming a single `data-theme` hook to `dioxus-workbench`.
