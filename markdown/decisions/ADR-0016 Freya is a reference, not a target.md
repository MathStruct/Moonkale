---
title: "ADR-0016 — Freya is a reference, not a target"
tags: [adr, editors, ui, interop]
status: accepted
date: 2026-10-04
---
**Status:** accepted 2026-10-04 (Daniel asked for the note after the Freya question came up in the Rust code editor work). Related: [[ADR-0001 Dioxus instead of Lumino and Tauri]], [[ADR-0010 dioxus-workbench for layout]], [[ADR-0011 Desktop graph surface strategy]], [[Rust-native Editor Candidates]], [[JS Interop Boundary]].

## Context
The Rust code editor work (GPT-6 Luna, `codex` worktree; notes *Rust Code Editor M0 Spike* / *Roadmap* on that branch) found **Freya**'s code editor (`freya-code-editor`, `freya_edit` 0.4): a rope model, virtual scrolling, pointer selection and IME preedit — the most complete Rust code editor around. Daniel asked what planning for Freya would mean, whether Moonkale's architecture leans against it, and whether an extension could ship on desktop but not on Android.

Facts, checked 2026-10-04:
- **Freya ≥ 0.4 is not Dioxus.** Freya 0.1–0.3 were built on Dioxus' core crates; from 0.4 it has its own reactive core (README). It draws with Skia in its own winit windows. Platforms: Linux, Windows, macOS, Android (recent). No web/wasm, no iOS mentioned.
- **Every Moonkale panel is a Dioxus `Element` in a webview**: `Extension::render` returns one, rendered into WebKitGTK (desktop), the browser (web) or Android's WebView. A Freya component cannot be returned there.
- **`moonkale-ext-api` is Dioxus-bound by design** (`Workspace` state is Dioxus signals, ADR-0010). **`moonkale-core` and `moonkale-graph-render` have no Dioxus dependency**; neither do the server, the stores, the sources.
- **Per-platform extensions already work**: `moonkale-distribution` has one Cargo feature per extension, each app (desktop, mobile, web) picks its features in its own `Cargo.toml` (as `julia` does), and `Extension::claims` plus the user's choice decide which editor opens a document.

## Ways Freya could come in
| Way | What it takes | Verdict |
|---|---|---|
| **Separate window/process** (desktop only) | An extension, built for desktop only, starts a Freya editor and swaps edits with the workspace over a channel or local socket. tao (Dioxus desktop) and winit (Freya) each want the process's single event loop, so in practice a second process | possible; two windows are worse than a tab |
| **Off-screen into a canvas in a tab** (like the graph view's wgpu canvas) | Forward input, focus, IME and clipboard by hand into Freya's renderer | no — that is exactly the hard part the editor work is fighting; it adds a second UI runtime for nothing |
| **A Freya shell on desktop** | A second shell, a second way to render extensions, a bridge from Dioxus signals to Freya's state; the web still needs the HTML shell | no — two front ends to maintain |

The architecture's real exit from the webview is **`dioxus-native` (Blitz)**: it renders the same elements and CSS without a webview, so extensions keep working. That is already the long-term direction ([[JS Interop Boundary]], ADR-0011) once the JS editors are replaced. Blitz is still experimental.

## Decision
1. **No Freya dependency and no Freya-shaped plans now.** Freya is the **behaviour reference** for the Rust editor: IME composition, clipboard, virtual scrolling, pointer selection.
2. **Editors keep a headless model.** Text, cursor, selection, undo and viewport live in plain Rust with no UI-framework dependency (the `editor-core` direction of the M0 spike); the Dioxus view only draws visible rows and turns events into commands. A later front end — Blitz, or Freya in a separate desktop process — then replaces the view, not the editor.
3. **Keep Dioxus out of model crates**, as `moonkale-core` and `moonkale-graph-render` already are. UI code belongs in extensions and the shell.
4. **Platform choice is a distribution feature**, not code in the shell: "the code editor on desktop is X, on Android Y" is a feature line in the app's `Cargo.toml`, with `claims` breaking the tie.

## Consequences
- Revisit if: Freya gains web/wasm support **and** an embedding API, or Blitz stalls while Freya's editor stays far ahead — then the separate-process route (way 1) is the one to test first, as a desktop-only extension.
- Open question, not checked: whether `freya_edit` can be used without the rest of Freya. If it can, its model could be compared with `editor-core` as the headless core of decision 2.
- [[Rust-native Editor Candidates]] lists Freya's editor.

Sources: [Freya README](https://github.com/marc2332/freya/blob/main/README.md), [`freya-code-editor` source](https://github.com/marc2332/freya/blob/main/crates/freya-code-editor/src/editor_ui.rs), [`freya_edit` API](https://docs.rs/freya-edit/0.4.0/freya_edit/).
