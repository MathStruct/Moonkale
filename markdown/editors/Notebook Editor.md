---
title: "Notebook editor (.ipynb) — plan"
description: A Jupyter notebook viewer/editor as an opt-in extension with no new dependencies, and kernel execution behind a Cargo feature (pure-Rust ZeroMQ) on desktop and server — Python via ipykernel, Julia via IJulia. Not built.
tags: [editors, notebook, python, julia, planning]
---
From [[Prompt19]] and the follow-up (2026-09-20): Daniel does not want dependencies of this kind in the core, but would not object to a notebook window safely behind an extension. The file and the kernel separate cleanly, so the extension can have the first without committing to the second.

## The file — easy, no new dependencies
`.ipynb` is JSON (nbformat 4): `cells[]` of type `markdown` / `code` / `raw`, each code cell with `execution_count` and `outputs[]` — `stream` (stdout/stderr text), `display_data` / `execute_result` (a MIME bundle: `text/plain`, `text/html`, `image/png` and `image/jpeg` as base64, `image/svg+xml`, `text/markdown`, `text/latex`, `application/json`), `error` (traceback) — plus `metadata.kernelspec` and `metadata.language_info` naming the kernel and language.

Everything needed to show it exists:
- markdown cells → the rich editor (with KaTeX, [[013]], and wiki-links, [[012]]);
- code cells → the code editor with the grammar from `language_info.name` (Python and Julia both highlight, [[010]]); the LSP can be attached later by writing the concatenated cells to a shadow file;
- outputs → text, inline images, SVG, KaTeX for `text/latex`, markdown through the rich renderer, and `text/html` in a **sandboxed `iframe srcdoc`** (the webview renders it; no JavaScript dependency of ours). `application/javascript` outputs are shown as source, never run.
- editing: cell text, add / move / delete / change type, clear outputs, **save back** with unknown fields preserved (`#[serde(flatten)] extra: Map` on cells, outputs and metadata — the same round-trip discipline as the flow files), `nbformat_minor` kept.

Size: about the table editor plus glue. Platforms: desktop, web, phone (viewing).

## Execution — medium; where the dependencies would be
Running a cell means the Jupyter kernel protocol: discover kernelspecs (`kernel.json` under `~/.local/share/jupyter/kernels`, `/usr/share/jupyter/kernels`, or `jupyter kernelspec list --json`), launch the kernel process with a connection file (ports, key, transport), and speak **ZeroMQ** on five sockets (shell, iopub, stdin, control, heartbeat) with HMAC-SHA256-signed JSON messages (`execute_request` → `execute_reply` + `stream` / `display_data` / `error` on iopub; `kernel_info_request`; `interrupt` / `shutdown`).

The one real dependency is ZeroMQ: the `zeromq` crate is **pure Rust** (no libzmq), so it lives behind a Cargo feature (`moonkale-ext-notebook/kernel`) that desktop and the server enable and the web client never does; HMAC/SHA-2/uuid are already in the tree. Around 1 500 lines. It gives Python via `ipykernel` and Julia via `IJulia` — also the honest path to the Julia REPL wished for in [[Julia and Lenticulum]], since a kernel *is* a REPL with rich output. The web client executes through the server like the terminal; the phone views only.

## Out of scope, stated up front
ipywidgets (need a JavaScript comm layer and kernel-side state — exactly the dependency to avoid); running `application/javascript` outputs; the `.py` percent-format notebooks (a later importer); collaborative editing of a notebook (the entity log covers the file like any other).

## Shape
`packages/extensions/notebook/` — opt-in ([[Extension Catalogue]]): an `editor` contribution for `.ipynb` (`is_notebook`), the cell list as a Dioxus panel reusing `CodeEditorPanel`/rich views per cell, a `NotebookBackend` trait with `View` (all platforms) and `Kernel` (feature `kernel`, desktop + server, wired like `spawn_terminal` through `WorkspaceConfig`). Cell outputs are cached by cell id; big base64 images fall under the size tiers of [[Case Selector]]. The agent gets `notebook.run_cell` as a tool under the policy gate (Mutating: it executes code).

## Steps (when scheduled)
1. nbformat types + round-trip test on a real notebook; the viewer (cells, outputs, sandboxed HTML).
2. Editing and saving; keyboard model (Esc/Enter, `A`/`B`, `Shift+Enter` = run when a kernel exists).
3. `kernel` feature: kernelspec discovery, launch, ZeroMQ session, execute/interrupt/restart, status in the toolbar; E2E with ipykernel on the server.
4. IJulia, LSP shadow file, the agent tool.
