---
title: "Milestone 6 — Scale & Extend: the plan"
description: Opt-in extensions with a runtime and permissions, the flow editor with a Lux.jl library as an optional extension, a faster graph layout with measurements, and a phone-sized shell.
tags: [milestone, planning]
---
**Goal** (from [[Roadmap]] Phase 6): *100k-node graphs; install third-party wasm extensions; build a Lux.jl model by drag-and-drop; use it on a phone.* Record: [[Milestone 6 - Implementation Log]].

**Daniel's constraint (2026-09-18):** anything like Lux.jl lives behind traits/extensions and is only loaded when a user wants it — never by default. This shapes step 1: *extension enablement* comes first, and the flow editor's block libraries are contributions from optional extensions.

## Starting point (after Milestone 5)
- Every extension is a static Rust crate registered in `ui::default_extensions()`; all of them are always on. `ext-host` is comment-only (`activation`, `permissions`, `registry`, `runtime`).
- `editors/flow` is comment-only; [[dioxus-flow]] 0.1.2 was evaluated and recommended as the canvas.
- Graph renderer: CPU force layout, O(n²) repulsion, ≤ 3 000 nodes drawn; WebGL2 (`gl`) on the Linux desktop, so no compute shaders there.
- Machine: `julia` 1.12 (no `Lux` package), no `cargo-component`, no Android SDK. `wasm32-unknown-unknown` target present.

## Scope: what "done" means
1. **Extensions are opt-in and managed** — a catalog of built-in extensions with `default_enabled`; the *Settings → Extensions* section lists them with toggles and the permissions each one asks for; disabled extensions contribute nothing (no panels, no commands, no tools); the choice persists (user scope, workspace override). Flow editor and Lux library ship **off**.
2. **Flow editor** (generic): `.flow.json` files open in a canvas (dioxus-flow): palette of blocks from enabled `FlowLibrary` contributions, drag to place, drag between typed ports to wire (type mismatch = red wire, rejected), parameters editable per block, save/load as ordinary nodes/edges JSON, visible in the Explorer and indexable.
3. **Lux.jl library** as an optional extension crate: `Dense`, `Conv`, `MaxPool`, `Flatten`, `Chain` (implicit by wiring), activations, `Input`, `Loss`, `Optimiser`; tensor-shape ports with a small unification; `flow.generate` command → `model.jl` (Lux `Chain` + a training scaffold) written next to the flow through the folder source; *Run* opens a terminal with `julia model.jl`. Verified by `julia`'s parser here; running needs `Lux` installed (asked from Daniel).
4. **wasm extension runtime v1** (`ext-host` + wasmtime, desktop/server): extensions are **core wasm modules** with a JSON ABI (`manifest`, `commands`, `run`), loaded from `~/.config/moonkale/extensions/*.wasm` and `<folder>/.moonkale/extensions/`; host imports `log`, `query(source, query)`, `fetch_text`, gated by **permissions** declared in the manifest and granted in Settings. Commands from wasm extensions appear as agent tools (`llm_tool`). An example extension (`extensions/wordcount`, plain `cargo build --target wasm32-unknown-unknown`) proves the loop. Components/WIT stay the documented next step.
5. **Scale**: Barnes–Hut repulsion (quadtree, θ = 0.8) in the renderer, edge drawing thinned and labels off beyond thresholds, a dev hook to generate synthetic graphs; **measured** at 10k / 50k / 100k nodes on this machine (WebGL2) and recorded. WebGPU compute is attempted only if the Barnes–Hut numbers say it is needed (stretch).
6. **Phone-sized shell**: below ~700 px the shell collapses to one tile with a bottom bar switching Explorer / Editor / Graph / Agent; touch-sized targets; the `mobile` crate builds and runs the same shell. Verified on web at 420 px (no Android SDK here; documented how to build the APK).
7. Web parity where it applies (extensions settings, flow editor, collapsed shell); E2E; documentation.

**Deferred** (documented): WIT/component extensions and the browser runtime (P-28), MTK library, execute-in-place Julia kernel, GPU compute layouts (if not needed), 3D (P-27), Postgres/Turso (P-19), TypeDB/Helix (P-26).

## Architecture decisions for this milestone

```mermaid
flowchart LR
  S[(settings.extensions)] --> CAT[ext-host catalog: built-in + wasm]
  CAT -->|enabled| SHELL[shell: panels/commands/tools]
  CAT --> FLOW[editors/flow canvas]
  LUX[ext: lux (optional)] -->|FlowLibrary| FLOW
  WASM[(~/.config/moonkale/extensions/*.wasm)] --> RT[ext-host runtime: wasmtime, JSON ABI, permissions]
  RT -->|commands| SHELL
  RT -->|tools| AGENT[agent]
```

- **Enablement is data, not code**: `Settings.extensions.enabled/disabled` overrides the catalog's `default_enabled`; `ui::default_extensions()` becomes the *catalog* and the shell filters by the resolved set every render, so toggling applies live.
- **Libraries are contributions**: `FlowLibrary { id, blocks: Vec<BlockKind>, codegen: Option<fn(&Flow) -> Result<String>> }` returned by `Extension::flow_libraries()` (new trait method, default empty). The flow editor never knows Lux exists.
- **dioxus-flow is the canvas**; our model (`Flow { blocks, wires }`) converts to its nodes/edges; validation happens on connect.
- **Flows are files** (`*.flow.json`) so they travel with the folder, index, and diff in git; the graph view can show them later through the index.
- **wasm v1 is a JSON ABI over core modules** because it needs no component tooling here; the ABI is versioned (`abi: 1`) and small enough to be replaced by WIT later without changing the manifest format.
- **Permissions are enforced at the host boundary**: each import checks the granted set; a denied call returns an error to the module, never a panic.
- **Barnes–Hut before compute shaders**: it works on every backend (WebGL2 included) and is where the time goes at 100k.

## Steps

| # | step | crates | verify |
|---|---|---|---|
| 1 | extension catalog + enablement in settings; Settings → Extensions; shell filters; `flow` + `lux` crates registered off | ext-api, ext-host, ui, editors/* | E2E: toggle → panel appears/disappears, persists |
| 2 | flow editor: schema, `FlowLibrary` contribution, dioxus-flow canvas, palette, typed wiring, params, `.flow.json` load/save, Explorer opens it | editors/flow, ui | unit: validation; E2E: place two blocks, wire, save, reopen |
| 3 | Lux extension: block library, shape unification, codegen, Run | extensions/lux, editors/flow | unit: codegen for a CNN; `julia` parses it |
| 4 | wasm runtime v1: ABI, loader, permissions, commands → palette + agent tools; example extension | ext-host, ext-api, editors/agent, extensions/wordcount | unit: run the example module; E2E: command visible only when granted |
| 5 | scale: Barnes–Hut, thresholds, synthetic-graph hook, measurements | graph-render, editors/graph | numbers in the log |
| 6 | phone shell: collapsed layout + bottom bar; mobile crate | ui, mobile | E2E at 420 px |
| 7 | verify, log, vault | | |

## Risks
| risk | mitigation |
|---|---|
| dioxus-flow 0.1.2 API gaps (typed handles, validation hooks) | validation on our side at connect time; if the crate blocks, an SVG fallback canvas is the plan-B (documented) |
| wasmtime compile time / size | `wasmtime` behind the `wasmtime` feature of `ext-host`; desktop and server only |
| Lux not installed | codegen is verified by the Julia parser; running is a user action with a clear install hint |
| 100k nodes still too slow on WebGL2 | thresholds keep the UI usable (no labels, sampled edges); the number is recorded, the compute path stays deferred |
| no Android device/SDK | the collapsed shell is tested at phone width on web and desktop; APK build documented, not run |
