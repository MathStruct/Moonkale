---
title: "Milestone 6 — Implementation Log"
description: What was built for "Scale & Extend", what deviated from the plan, the measurements, and the problems hit.
tags: [milestone, log]
---
Plan: [[Milestone 6 - Scale and Extend]].

> [!success] Done (2026-09-19)
> All seven steps are implemented and verified on the web build; desktop and mobile compile and share every code path except the wasm runtime's placement. **Extensions are a managed catalog**: every built-in declares `core` / `optional` / `opt_in` and the permissions it wants; *Settings → Extensions* toggles them and grants permissions; the shell drops disabled extensions live (no panels, no libraries, no tools). The **Flow editor** (opt-in) opens `*.flow.json` on a [[dioxus-flow]] canvas with a palette from enabled `FlowLibrary` contributions, typed ports (tensor-shape unification), per-block parameters, and a *Generate* button. The **Lux.jl library** (opt-in) contributes Input / Dense / Conv / MaxPool / Flatten / Dropout / BatchNorm / Loss / Optimiser blocks and writes `model.jl` (a Lux `Chain` + training scaffold) next to the flow — verified with `julia`'s parser. **wasm extensions v1**: core wasm modules with a JSON ABI (`manifest` / `run`, host `log` / `call`), loaded by wasmtime from `~/.config/moonkale/extensions/*.wasm` and `<folder>/.moonkale/extensions/`, permission-checked at every host call, listed in Settings (off by default), their commands offered to the agent as tools; `extensions/wordcount` is the example. **Scale**: Barnes–Hut repulsion (θ = 0.8) from 1 500 nodes, iteration caps and label LOD — 100k nodes lay out at 169 ms/step natively and 189 ms/step in Firefox's wasm (O(n²) extrapolates to ~12 s/step). **Phone-sized shell**: below 700 px the workbench collapses to one tile with a bottom bar (Files / Search / Editor / Graph / Terminal / Agent / Settings), verified at 420 px. Three new E2E suites (`flow`, `wasm-ext`, `phone`) plus all fourteen earlier ones pass; 64 native tests; clippy/fmt clean.

## Steps as executed

| # | step | outcome | notes |
|---|---|---|---|
| 1 | `Manifest::{core, optional, opt_in}` + `with_permissions`; `ExtensionsFile`/`ExtensionsSettings` in settings (enabled/disabled/permissions, user ← workspace overlay per id); shell filters; Settings → Extensions | ✅ 1 unit test, E2E (`flow.mjs` toggles the flow editor on) | [ext-api.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-api/ext-api.md), [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 2 | `ext-api::flow` (PortType/unify, Param, BlockKind, FlowLibrary, Flow JSON, validate, topological); `Extension::flow_libraries()`; `editors/flow` on dioxus-flow 0.1.2; File → New Flow…; Explorer opens `.flow.json` | ✅ 2 unit tests, E2E `flow.mjs` | [flow.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/flow/flow.md) |
| 3 | `extensions/lux`: block library, shape unification through the ports, Julia codegen with a lazy `flat_features`, *Generate* writes `model.jl` via the folder source | ✅ 3 unit tests incl. `julia -e Meta.parseall` | [lux.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/lux/lux.md) |
| 4 | `ext-host`: JSON ABI v1 (`abi.rs`), wasmtime 48 runtime with permission-checked host calls (`runtime.rs`), `discover`; desktop in-process, web through server functions; commands → agent tools; `extensions/wordcount` example + `build.sh` | ✅ integration test builds and runs the module; E2E `wasm-ext.mjs` | [ext-host.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ext-host/ext-host.md), [wordcount.md](https://github.com/MathStruct/Moonkale/blob/master/packages/extensions/wordcount/wordcount.md) |
| 5 | Barnes–Hut quadtree, iteration caps, label LOD, `bench_layout` export, `GRAPH_LIMIT` 100 000, native `bench_layout` example | ✅ numbers below | [graph-render.md](https://github.com/MathStruct/Moonkale/blob/master/packages/editors/graph-render/graph-render.md) |
| 6 | phone shell: `narrow` from `matchMedia`, single-tile controlled layout, bottom bar, no persistence while narrow; `mobile` crate builds | ✅ E2E `phone.mjs` at 420 px | [ui.md](https://github.com/MathStruct/Moonkale/blob/master/packages/ui/ui.md) |
| 7 | verify, log, vault | ✅ | this note |

## Measurements (step 5)

Force layout, ms per step, synthetic graph (`n` nodes, ~1.5 n edges), this machine (Ryzen 7 7800X3D, single thread):

| nodes | native (`cargo run --release --example bench_layout`) | wasm in Firefox (`bench.mjs`) | O(n²) extrapolated from 1k |
|---|---|---|---|
| 1 000 | 1.2 | 1.8 | 1.2 |
| 10 000 | 12.9 | 15.3 | 120 |
| 50 000 | 79.4 | 85.7 | 3 000 |
| 100 000 | 169.3 | 189.3 | 12 000 |

Barnes–Hut turns the step from quadratic to n·log n; wasm costs ~10 %. Drawing 100k nodes is not the problem (two instanced draws); the layout is, and at 120 iterations (the cap above 50k) a 100k graph settles in ~20 s in the browser, ~17 s natively. WebGPU compute is therefore **not needed** for this milestone (plan: only if Barnes–Hut fell short) and stays deferred. Labels switch off above 20 000 nodes; the panel asks sources for at most 100 000 nodes.

## What the user sees
- **Settings → Extensions** lists every built-in with a toggle and its permissions; the Flow editor and the Lux library are **off** until switched on (per machine or per folder — `.moonkale/settings.json` carries `extensions.enabled` / `disabled` / `permissions`). wasm modules found on disk are listed underneath, off by default, with permission checkboxes.
- With the flow editor on: **File → New Flow…** creates `untitled.flow.json`; the palette (left) shows the enabled libraries' blocks; click a block to place it, drag from an output handle to an input handle to wire (mismatched shapes are rejected), edit parameters in the block, **Generate** writes `model.jl` next to the flow (Lux), **Layout** auto-arranges, **Save** (Ctrl+S) stores the JSON.
- Install the example wasm extension: `packages/extensions/wordcount/build.sh` (needs `rustup target add wasm32-unknown-unknown`) copies `wordcount.wasm` to `~/.config/moonkale/extensions/`; enable it in Settings → Extensions, grant *read-sources*, then ask the agent to "count the words in README.md" — the `wordcount.count` tool appears with an approval card (third-party tools are treated as mutating).
- On a phone-sized window (< 700 px wide) the rail disappears and a bottom bar switches between the panels; widening the window brings the saved multi-tile layout back.

## Deviations from the plan
1. **Extension enablement stayed in `ext-api`** (`settings::ExtensionsSettings`), not in `ext-host`: the shell and the Settings panel already read `Settings`; `ext-host` now contains only the wasm runtime and ABI, behind the `wasmtime` feature (desktop and server), so the web wasm build never compiles wasmtime.
2. **The flow editor keeps one wire per input port** and validates at connect time with `PortType::unify` (rank-polymorphic tensors: `Tensor(Any)` unifies with any rank); dioxus-flow's own `is_valid_connection` hook was enough, so no plan-B SVG canvas.
3. **Flows are files** (`*.flow.json`, `version: 1`); the code editor skips them (`CodeEditorExtension::skipping(is_flow)`) the way it skips markdown, so the flow tab uses the shared `editor:<uuid>` panel id and closing/saving/dirty work unchanged.
4. **`Generate` is a toolbar button, not a command palette entry** (there is no palette yet); `Run` is a hint in the status line (`julia model.jl`) rather than an auto-opened terminal — Lux is not installed here, so the terminal would only show the install message.
5. **wasm ABI v1 is core modules + JSON** (`alloc`, `manifest`, `run` exports; `moonkale.log` / `moonkale.call` imports; packed `(ptr << 32) | len` returns). No WIT, no component model, no browser runtime: documented as the next step in [[Extension System]]. Permissions are checked per host call in the host, never trusted from the module.
6. **wasm on web runs on the server** (`list_wasm_extensions`, `run_wasm_command` server functions, granted permissions passed per call from the client's settings); the browser never loads wasmtime. Discovery uses the server's config dir (`MOONKALE_CONFIG_DIR`) and the open folder.
7. **Third-party tools are `Mutating` by policy**: an unknown tool name is never classified read-only, so a wasm command always asks unless *Allow mutating tools* is on. The plan said "commands become tools"; it did not say they would be trusted.
8. **The phone shell is a mode of the same `Shell`**, not a second component tree: two `PanelWorkspace` branches, each with its own controlled signal (the workbench needs one signal per lifetime), `home_tile()` maps every contribution to `main` while narrow, and `on_layout_change` is ignored so the phone arrangement never overwrites the saved desktop layout. Switching modes remounts the editors (acceptable: it happens on a resize, not while typing).
9. **No Android build** (no SDK here): `cargo check -p mobile --features mobile` passes and the shell is the same; the APK recipe stays in `packages/mobile/README.md`.

## Problems hit (→ [[Problem Log]])
- **P-073 dioxus-flow handle geometry lags the auto-layout animation**: after *Layout*, `Handle` positions used for connection validation are stale for a few frames, so a drag started right after a layout can miss. The E2E wires before laying out; a real fix needs a settled-layout callback from dioxus-flow (not yet requested upstream). Related: the third palette column sat under the tile splitter at 1500 px, so blocks are placed on a 165 px grid.
- **P-074 The workbench renders a reconciled copy of a controlled layout**: newly attached panels (and the auto-activated document tab) exist in the rendered tree before they are written to the application's signal, so reading "which tab is in front" from the signal was stale. The phone bar reconciles the signal with the current placements before reading or activating, exactly as `ShowPanel` does.
- **P-059 (fixture drift) is closed**: `packages/web/tests/e2e/fixture.sh` builds the fixture folder deterministically (users table, small `people.lbug` via `seed_people … small`), and the regression script resets `Home.md`, `.moonkale/` and generated files between suites. The scratchpad this session used was wiped at the day change, which is what forced the script into existence.
- **P-047 again** (desktop): the narrow-shell watcher (`matchMedia` eval) was created in a hook and never reported on desktop; started from the shell's `onmounted` now, and the desktop window's minimum width is 360 px so the phone layout can be tried there.
- **P-070 again**: assets edited without a Rust change are served stale by `dx serve` — touch a `.rs` in the crate that owns the asset and let dx rebuild (the hash is computed at compile time).
- `mistral-code-latest` was not needed this milestone; all suites run on the mock provider (`MOONKALE_LLM=mock`).

## Decisions worth keeping
- **Enablement is data**: `default_enabled` in the manifest, overridden by `extensions.enabled/disabled` per scope; the shell filters every render, so a toggle applies live without a restart.
- **Libraries are contributions**: the flow editor never knows Lux exists; a second library (MTK, a data-pipeline set) is another `FlowLibrary` from another extension.
- **The ABI is versioned and boring** (`abi: 1`, JSON strings both ways) so it can be replaced by WIT without changing the manifest format.
- **Permissions are enforced at the host boundary**, and third-party commands are policy-`Mutating` until proven otherwise.
- **Barnes–Hut before compute shaders**: works on WebGL2 and on the desktop's GL path, and it removed the wall.
- **Narrow mode is not a layout to save.**

## Verified
- Web (Firefox, Playwright, mock provider, port 8090, fresh fixture per suite): `milestone1`, `menubar`, `session`, `graph`, `links-sqlite`, `terminal`, `typst`, `lsp`, `ladybug`, `agent`, `search-trace`, `settings`, `rich`, `agent-writes`, `flow`, `wasm-ext`, `phone` — all PASS.
- Native: `cargo test --workspace --features moonkale-sources-graph/ladybug,moonkale-ext-host/wasmtime` → 64 passed, 0 failed (3 ignored: live services); `cargo clippy --workspace --all-targets` clean; `cargo fmt` clean; `cargo build -p desktop --features desktop` and `cargo check -p mobile --features mobile` pass.
- Julia: the generated `model.jl` for a CNN parses (`Meta.parseall`); running it needs `Lux`, `Optimisers`, `Zygote`.

> [!success] Confirmed on desktop by Daniel (2026-09-19)
> The shell switches to the phone layout once the window goes below the width threshold, and the agent runs the wasm `wordcount` command after the extension was enabled and granted *read-sources*. (First attempt did nothing: the media-query watcher was created from a hook, which is lost on desktop — P-047 — and the window's minimum width was 640 px; both fixed.)

## What to look at on desktop
1. `Ctrl+,` → Extensions: switch **Flow editor** and **Lux.jl blocks** on; File → New Flow…; place Input → Conv → MaxPool → Flatten → Dense → Loss, wire them, Generate, open `model.jl`.
2. `packages/extensions/wordcount/build.sh`, restart the app, Settings → Extensions → enable *Word count (wasm example)* and tick *read-sources*; ask the agent for a word count.
3. Open a `.lbug` with many rows, or a big folder: the graph settles instead of freezing; labels appear once you zoom in.
4. Resize the window below 700 px: the bottom bar appears; widen it again.

## Deferred to Milestone 7
WIT/component extensions and the browser runtime (P-28), an MTK/data-pipeline flow library, execute-in-place Julia (run the generated model in a terminal with output back into the flow), GPU compute layouts (not needed at 100k), 3D graph (P-27), Postgres/Turso (P-19), TypeDB/Helix (P-26), an Android build on a machine with the SDK, OS keychain, a cancel for running tools (P-069), settled-layout callback in dioxus-flow (P-073).
