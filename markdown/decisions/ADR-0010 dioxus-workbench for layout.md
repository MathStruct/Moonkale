---
tags: [adr]
status: accepted
date: 2026-09-17
---
# ADR-0010 — dioxus-workbench for layout

**Status:** accepted

## Context
Lumino's docking was the reason the old plan chose it. Dioxus needed an equivalent: tabs, splits, drag-to-dock, persistence, accessibility.

## Decision
Use `dioxus-workbench` 0.1 (renderer-agnostic, serde layouts, `--wb-*` theming, ARIA). The shell (`ui`) owns the workspace; extensions contribute `Panel`s.

## Consequences
- Verified in the dummy UI ([[Prompt1]]): drag-to-dock, splits, status bar, rail work on web/desktop.
- Known limitation: panel content remounts on structural moves → state must live outside panels (documented for extension authors).
- Missing: floating panels, maximise, tab overflow — on the crate's roadmap; we may contribute.
- Mobile needs a collapsed shell wrapper on top.
