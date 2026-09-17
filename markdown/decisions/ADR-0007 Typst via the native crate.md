---
tags: [adr]
status: accepted
date: 2026-09-17
---
# ADR-0007 — Typst via the native crate

**Status:** accepted

## Context
Typst is written in Rust and published as the `typst` crate; it compiles to wasm.

## Decision
Compile Typst in-process on every platform with a `World` backed by the folder source. Render to SVG for preview. No JS.

## Consequences
- Zero interop cost; works offline on mobile.
- Fonts/packages need a cache strategy per platform (OPFS on web).
- A structured Typst editor is future research (`typst-syntax` exposes the AST).
