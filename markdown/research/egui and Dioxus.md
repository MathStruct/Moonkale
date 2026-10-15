---
title: "egui and Dioxus — what egui would bring, what it would cost, and how to compare them cheaply"
description: Daniel asked (2026-10-14) what the benefit of egui would be, and whether comparing it with Dioxus later would be too troublesome. egui's strengths, why it doesn't fit Moonkale's text-heavy, web-first parts, why a switch gets more expensive over time and a comparison does not, and a small spike to run if the question comes back. Companion to ADR-0016 (Freya).
tags: [research, ui, egui, dioxus]
---
**Context:** Moonkale's UI is Dioxus ([[ADR-0001 Dioxus instead of Lumino and Tauri]]), rendered in a webview on the desktop and Android and in the browser on the web. [[ADR-0016 Freya is a reference, not a target]] settled the same question for Freya. egui came up because the ArtCraft suite (clean-room Rust rewrites of desktop apps) is built on it.

## What egui would bring

- **Pure Rust, no webview.** No WebKitGTK quirks (Moonkale's desktop already works around several, P-001 onward), a smaller desktop footprint, and the same pixels on every platform.
- **Fast to build tool UIs.** Immediate mode: the UI is a function of state, redrawn on input. Panels, inspectors, property grids and debug views come together quickly.
- **A mature ecosystem for the kind of tools Moonkale has:** `egui_dock` (docking), `egui_extras` (virtual tables), `egui_plot` (plots), several node-graph editors (`egui_snarl` and others), `egui_graphs`.
- **Custom GPU drawing is first class:** a wgpu paint callback draws inside any egui region. Moonkale's graph engine (UI-free since ADR-0018) could draw inside an egui app, which is the interop point with ArtCraft.
- **Testable UIs:** `egui_kittest` drives an egui UI in tests through its AccessKit tree, headless, without OS input injection. That's exactly what Moonkale lacks on native Wayland.
- **Web from the same code,** as one canvas.

## What it would cost Moonkale

- **Text is where Moonkale lives, and the browser's text engine is the strongest one available:**
  - IME composition (Chinese and Japanese input), complex scripts, bidirectional text, font fallback for CJK;
  - selection and find across a page;
  - rich WYSIWYG Markdown;
  - spell checking.

  egui has IME support and is improving, but it doesn't match a browser here. The Markdown rich editor (Milkdown), CodeMirror and xterm are DOM components with no egui equivalent.
- **The web as a canvas loses what makes a web app a web app:** native text selection, browser find, links you can open in a new tab, screen readers reading the DOM, crawlability (relevant to reader mode, [[Publishing Sources]]).
- **Theming and layout:** spec 030's themes are CSS variables. egui's styling is code, and responsive layouts are harder in immediate mode.
- **Every extension's UI is Dioxus.** `Extension::render` returns a Dioxus `Element`, and `Workspace` state is Dioxus signals. A switch means rewriting the shell and every panel, more than 40 crates' UI. The model crates (`moonkale-core`, `moonkale-graph-render`, the sources, the server, the state store) have no UI dependency and would survive.

## Is comparing them later troublesome?

**Comparing: no. Switching: yes, and more so every month.** The comparison is cheap because the parts that matter are UI-free and benchmarks exist:
- **A spike** (one agent, a small package): an egui app showing **one real panel twice** (the virtualised table grid on a 50 000-row result, and the graph engine through a wgpu paint callback), reading the same model crates, beside the Dioxus panels.
- **Measured with [[Benchmarks]]:**
  - frame times while scrolling and panning (BM-11, BM-14);
  - memory (BM-20);
  - the web download (BM-30);
  - Chinese IME input;
  - screen-reader output;
  - how long each took to build.

The switch, not the comparison, is what grows expensive. The rules that keep the option open are already in place:
- **models are headless** (ADR-0016 decision 2);
- **logic lives outside components** (the table's `grid.rs`, the flow model in `ext-api`, the Markdown parser in `markdown.rs` are pure and tested);
- **Dioxus stays out of model crates.**

Every package that follows them keeps a later UI change a rewrite of views, not of Moonkale.

## Verdict

- **No switch.** Moonkale is text-heavy and web-first, and that is where Dioxus with a real browser engine is strongest. The planned exit from the webview stays **Blitz** (`dioxus-native`), which renders the same elements and CSS without a webview.
- **egui is an interop target:** the graph engine embedded in egui apps (ArtCraft's node editors), and `egui_plot` and `egui_kittest` as references for the plot extension and for UI testing.
- **Revisit if** Blitz stalls, **or** egui's text input reaches browser quality (IME, bidi, CJK), **or** Moonkale drops the web client. Then run the spike above first.

Work package (only if revisited): WP-58 in [[Work Packages]].
