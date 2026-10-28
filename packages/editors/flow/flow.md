---
title: "editor-flow — implementation notes"
tags: [crate-notes, milestone-6, spec-031]
---
Notes for `moonkale-editor-flow` (Milestone 6; the engine host is spec 031 stages 7b–7c, ADR-0018). Design: [[Flow Editor]].

- **Opt-in extension** (`Manifest::opt_in("dev.moonkale.editor-flow", …)` with `write-files`): off until enabled in Settings → Extensions; the shell filters it out otherwise, and File → New Flow… only shows when it is on.
- **Model lives in `ext-api::flow`** (frontend-agnostic — moonkale-julia depends on it, so its public types stay stable): `PortType` (`Tensor(Shape)` with `Any`-rank unification, `Scalar`, `Custom`), `Port`, `Param`/`ParamKind`, `BlockKind`, `FlowLibrary { id, name, blocks, codegen, language }`, `Flow { version, blocks, wires }` (the `.flow.json` shape), `validate` (missing kinds, type mismatches, unwired required inputs, cycles), `topological`, `find_kind`. The flow editor never names a library.
- `extension.rs`: `is_flow(node)` = `*.flow.json`; the code editor skips those nodes (`CodeEditorExtension::skipping(is_flow)` in `ui`), so the flow tab uses the shared `editor:<uuid>` panel id and open/close/dirty/Save work as for any document.
- `panel.rs` — `FlowPanel { ws, node }` hosts the graph engine like the graph panel does (`moonkale-graph-assets`' `RENDER_JS`/`RENDER_WASM`, `window.moonkale.flowViews[ID]`, `set_interaction("edit")`):
  - `to_scene` converts `Flow` → the engine's wire format: rect block nodes (size by port count, a hue per kind, issue-red), ports on the left/right coloured by `PortType`, directed ortho wires, every node **pinned** — the document owns the coordinates.
  - The engine emits `connect`; the panel validates with `unify` (`wire`), one wire per input port, and adds the wire — a refusal shows a notice and re-pushes the scene without the gesture's wire. The renderer never mutates the model.
  - **Acausal pins (ADR-0020):** a `BlockKind`'s `connections` are undirected ports, domain-typed (`PortType::Named`). A wire between two pins connects in either drag order, a pin may carry several wires (a bus — `connect(p, n, q)`), pin loops are legal (equations, not dataflow: `validate` runs the cycle check over directed wires only), and required pins are satisfied from either direction. The scene draws pins on the block's top edge (`direction: "both"`, the engine's own default) and pin wires without arrowheads. Lux has no pins; the ModelingToolkit rung will.
  - `moved` (drag end, Layout) harvests the engine-reported positions back into the document; a restore (undo/redo) re-sends the scene and the engine moves the pinned blocks to the restored coordinates (a pinned node with `pos` is the host's to move — `bounds_with_rims`' sibling in `web.rs`).
  - **Undo/redo (stage 7c):** the working copy's JSON before each mutation (palette add, wire, drag, parameter, delete) on a 100-deep stack; Ctrl+Z / Ctrl+Y (Ctrl+Shift+Z), and a new mutation empties the redo stack. A field's own Ctrl+Z stops at the field (spec 001) — native text undo. An external reload (document replaced) empties both stacks.
  - **Parameter editor (spec 031 §7):** one HTML editor for the selected block, floating beside it on the canvas — a world-space `canvas-layer` card following camera reports, with LOD unmounting and a draggable header. No per-node DOM.
  - Palette (left) from `ws.contrib.flow_libraries` (read at use time — a library enabled later is addable at once); blocks placed on a 165 × 170 grid, 3 columns; each button carries its own `onclick` with the kind captured, keyed (the delegated read of `data-kind` from `web_sys` crashed off the web — desktop and Android run their handlers outside wasm; P-167); toolbar: **Generate** (the library's `codegen`, written next to the flow through `Workspace::node_at_path` / `create_text`, then opened), **Layout** (`hier_layout`), **Fit**, **Save** — all but **Reload** disabled while the document does not parse. `mk-flow-status` carries `data-blocks/wires/issues` for tests.
  - The engine focuses the overlay on pointerdown (capture phase — stage 10 installs it for every host): the engine's own pointerdown prevents the default, which would otherwise leave the keyboard focus wherever it was — the panel's keys (Delete, Ctrl+Z, Ctrl+S) then never fire.
- `assets/flow.css`: palette, canvas, the floating editor, status — the dioxus-flow classes are gone (stage 7c).
- **Parse errors (P-156, issue #11):** a document that does not parse is an error state (toolbar + message, no canvas or palette), never an empty canvas — an empty canvas would serialise itself over the file on the first change. The document is left untouched; fixed text (reopen, or a reload) brings the canvas back.

E2E: `packages/web/tests/e2e/flow.mjs` — enable both extensions in Settings, New Flow…, place the seven Lux blocks, the spec 001 keyboard guard in a parameter field, six typed wires (a mismatch refused with a notice), the editor floating beside its block (positioned in world coordinates through the shared camera transform), undo/redo of a wire, a drag (the block returns to the document's coordinates), a parameter and a delete, Layout + Save, Generate → `model.jl`, disabling Lux → unknown-kind issues, and the broken-file guard (#11).

## Canvas component layer (WP-05, 2026-10-22)

The selected block's parameter editor now uses `moonkale-canvas-layer`, anchored
in world coordinates beside the block. One CSS camera transform follows pan,
zoom, fit and resize. The card unmounts below 100 × 32 projected CSS pixels or
outside the viewport; the schematic block remains. Fields retain spec 001's
keyboard guard and do not select the canvas underneath. Dragging the card header
updates the block's document coordinates and is one undo step; rotation/mirror
buttons retain their normal actions. The layer is shared infrastructure outside
`editors/`, with no extension assembly dependencies. `canvas-layer.mjs` covers
camera following, click isolation, LOD, header movement and undo/redo.

Automatic flow fits cap magnification at 1× (lower for a narrow viewport) so a
single node leaves room for its world-space card. Explicit zoom remains free.
Undo/redo retain only selections present in the restored scope, matching the
engine's id-preserving graph replacement and keeping header-drag undo usable.

P-227: Generate replacements use the fetched text/version pair, preserving source conflict checks. The MTK browser suite covers creating the RC model and overwriting it with the mixed model.

If LOD removes a captured header before pointer-up, the host closes that gesture at its last world position. Its snapshot stays one undo step and a remounted header starts a new gesture. Header dragging does not auto-fit the camera under the pointer.


WP-64 (2026-10-28): Save uses the shared file.save command binding for its tooltip and hold-Ctrl badge.


WP-61 shares the engine's focused left-hand camera controls and preserves
flow's R/M orientation actions. Middle dragging always pans, including over
ports/nodes. The host ignores the engine's background-open event for flow
blocks. F19 (`RunDocument`) generates with the first applicable library, using
the active document only; the normal generator validation/save path still runs.
