---
title: "editor-flow — implementation notes"
tags: [crate-notes, milestone-6]
---
Notes for `moonkale-editor-flow` (Milestone 6). Design: [[Flow Editor]], canvas: [[dioxus-flow]] 0.1.2.

- **Opt-in extension** (`Manifest::opt_in("dev.moonkale.editor-flow", …)` with `write-files`): off until enabled in Settings → Extensions; the shell filters it out otherwise, and File → New Flow… only shows when it is on.
- **Model lives in `ext-api::flow`** so libraries can be contributed by other extensions: `PortType` (`Tensor(Shape)` with `Any`-rank unification, `Scalar`, `Custom`), `Port`, `Param`/`ParamKind`, `BlockKind`, `FlowLibrary { id, name, blocks, codegen, language }`, `Flow { version, blocks, wires }` (the `.flow.json` shape), `validate` (missing kinds, type mismatches, unwired required inputs, cycles), `topological`. The flow editor never names a library.
- `extension.rs`: `is_flow(node)` = `*.flow.json`; the code editor skips those nodes (`CodeEditorExtension::skipping(is_flow)` in `ui`), so the flow tab uses the shared `editor:<uuid>` panel id and open/close/dirty/Save work as for any document.
- `panel.rs` — `FlowPanel { ws, node }`: `to_canvas` / `to_flow` convert between `Flow` and dioxus-flow `Node<BlockData>` / `Edge`; handle ids are port names; `is_valid_connection` unifies port types (one wire per input port); every change writes `doc.text` (the document is the source of truth, Ctrl+S saves). Palette (left) from `ws.flow_libraries`; blocks are placed on a 165 px × 170 px grid, 3 columns; parameters edit in the node view; toolbar: **Generate** (runs the library's `codegen`, writes the file next to the flow through `Workspace::node_at_path` / `create_text`, opens it), **Layout**, **Fit**, **Save**. `mk-flow-status` carries `data-blocks/wires/issues` for tests.
- `assets/flow.css`: theme variables on `.mk-flow-canvas .dioxus-flow`, handles above block content (P-073 note).

Known: dioxus-flow's handle geometry lags its layout animation (P-073) — wire before pressing *Layout*, or wait a moment. No copy/paste, no undo inside the canvas yet (the document's undo covers the JSON).

E2E: `packages/web/tests/e2e/flow.mjs` — enable both extensions in Settings, New Flow…, place Input/Conv/Dense/Loss, a rejected mismatch (Conv → Dense), valid wires, Generate → `model.jl` opens, Save → file on disk.
