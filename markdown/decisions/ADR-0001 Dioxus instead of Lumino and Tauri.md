---
tags: [adr]
status: accepted
date: 2026-09-17
---
# ADR-0001 — Dioxus instead of Lumino + Tauri

**Status:** accepted

## Context
The original plan ([[old/rough goal]]) was Deno + Vite + Lumino (TS) for the frontend and Tauri + Rust for the backend. The brief now asks to stay in the Rust ecosystem as far as possible, serve desktop, web and mobile, and be extension-driven with a Rust-facing API.

## Decision
Use **Dioxus 0.7** for the whole application: one Rust codebase, three targets (`desktop` webview, `web` wasm, `mobile`), server functions for the backend. Lumino's docking is replaced by `dioxus-workbench` ([[ADR-0010 dioxus-workbench for layout]]). TypeScript survives only as isolated view packages ([[ADR-0002 Rust first, TypeScript behind traits]]).

## Consequences
- One language, one type system across UI, model, sources and extensions — the extension API can be a Rust trait.
- Mobile comes almost for free; with Tauri it was a separate effort.
- Lose the TS ecosystem's breadth; mitigated by the interop boundary.
- Dioxus 0.7 is younger than Lumino; expect to contribute upstream (the workbench crate already exists because of this).
- The desktop app is still a webview, which creates the graph-surface problem ([[ADR-0003 wgpu for graph rendering]]). The escape hatch (`dioxus-native`) requires no JS dependencies — see [[JS Interop Boundary]].
