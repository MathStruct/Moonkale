---
title: "ADR-0004 — WASM components for extensions"
tags: [adr]
status: accepted
date: 2026-09-17
---
**Status:** accepted

## Context
Extensions must be distributable, sandboxed, and portable across desktop/web/server. Options in [[WASM Extension Runtimes]]: native dylibs (unsafe, platform-specific), scripting (Lua/JS — a second language), WASM core modules (`extism`), WASM components (WIT).

## Decision
Two runtimes, one API: **static** Rust crates (built-ins, full Dioxus) and **WASM components** implementing a WIT world generated from `ext-api`. `wasmtime` on desktop/server; `wasm_component_layer` in a Worker on web; not on mobile in v1.

## Consequences
- Sandboxing and capability enforcement are natural.
- WIT gives non-Rust extension authors a path.
- Panels from wasm use a declarative `ui::Tree`, not Dioxus — less expressive, but portable and cheap over a Worker.
- Browser-side component hosting is the least mature piece; ranked very high difficulty in [[Problem Ranking]].
