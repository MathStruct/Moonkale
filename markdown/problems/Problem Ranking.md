---
title: "Problem Ranking"
tags: [problems, planning]
---
Difficulty 1–5 (effort + unknowns), Risk = how much else breaks if this goes wrong. **Order** is the recommended implementation order, justified in [[Roadmap]]. Each problem gets a `P-nnn` note in [[Problem Log]] when work starts.

| # | Problem | Diff | Risk | Order | Why this position |
|---|---|---|---|---|---|
| P-01 ✅ | Core graph model + `Source` trait + query IR | 3 | **very high** | 1 | Everything depends on it. Get it wrong and every crate churns. Design it with the folder + SQLite sources in hand. |
| P-02 ✅ | Folder source (native) + explorer panel | 2 | low | 2 | First real source; cheap; unblocks the code editor. |
| P-03 ✅ (web verified; desktop compile only) | JS interop protocol (`eval` + events) on all 3 webviews | 3 | medium | 3 | Mobile/WebKitGTK quirks unknown; must be proven before three editors depend on it. |
| P-04 ✅ | Code editor with CodeMirror behind `CodeEditorBackend` | 3 | medium | 4 | First editor; validates P-03 and [[ADR-0008 Rust owns the document, JS is a view]]. |
| P-05 ✅ (minimal: panels only) | Extension API + static registry; built-ins become extensions | 3 | **high** | 5 | API lock-in. Doing it *after* one real editor exists keeps it honest; before the second editor keeps it from being retrofitted. |
| P-06 ✅ (native tree-sitter, not wasm grammars) | tree-sitter index + wiki-link extraction | 3 | medium | 6 | Turns files into a graph — the first moment the "graph-native" promise is visible. |
| P-07 🔶 built, rendering unverified on this machine | Graph view v1: wgpu 2D, WebGL2+WebGPU, CPU layout, pick/popup | 4 | **high** | 7 | Flagship; needs P-06 for interesting data. ≤10k nodes target. |
| P-08 | SQLite + DuckDB sources + table editor | 3 | low | 8 | Embedded, no server; proves SQL lifting; DuckDB gives CSV folders. |
| P-09 | Markdown editor: source mode, links, backlinks, local graph | 2 | low | 9 | Mostly composition of P-04, P-06, P-07. |
| P-10 ✅ (folder only, no auth) | Remote source via `api` (web/mobile parity) | 3 | medium | 10 | Auth + streaming; first time the server matters. |
| P-11 | Milkdown WYSIWYG behind `RichTextBackend` | 3 | medium | 11 | Second interop package; round-trip fidelity is the risk. |
| P-12 | Typst preview | 2 | low | 12 | Pure Rust; `World` impl over folder source. |
| P-13 | Terminal: PTY + xterm view + links | 2 | low | 13 | Contained; high user value. |
| P-14 | LSP client + local spawning | 3 | medium | 14 | Protocol is known; document sync + neutral feature types is the work. |
| P-15 | Graph DB sources: Ladybug, Falkor | 2 | low | 15 | Natural fit; embedded Ladybug first. |
| P-16 | Cross-source edges + consistency (dangling targets, versions) | 4 | **high** | 16 | Needed once two sources are open at once and pages link to rows. Deferred until real cases exist. |
| P-17 | LLM gateway: providers, tool surface, policy, audit | 3 | medium | 17 | Composition over the command bus; policy classification depends on P-08's SQL classifier. |
| P-18 | Embeddings + hybrid search | 3 | medium | 18 | Needs P-17 + P-06 chunking; storage choice (`usearch` vs source `VECTOR`). |
| P-19 | Postgres/Supabase, Turso sources | 2 | low | 19 | Same lifting as SQLite; networked. |
| P-20 | Remote LSP + remote terminal on `api` (security) | 4 | **high** | 20 | Code execution as a service: auth, jail, limits, allow-lists, audit. Do not ship earlier. |
| P-21 | Stack-trace / AST → graph | 2 | low | 21 | Small parsers; hierarchical layout from P-07. Great demo. |
| P-22 | GPU compute layouts; 100k+ target; desktop surface decision ([[P-001 Graph surface in desktop webview]]) | 5 | **very high** | 22 | Compute shaders + platform surface research. Needs P-07 stable and measurements. |
| P-23 | wasmtime extension runtime + WIT world + permissions UI | 4 | **high** | 23 | After the API has settled through several static extensions. |
| P-24 | Flow editor + Lux.jl codegen | 3 | medium | 24 | Independent; needs P-05 (libraries as contributions) and P-13 (run). |
| P-25 | Mobile: file access, collapsed shell, touch | 4 | medium | 25 | Platform work; UX not primary coding. Do after web parity (P-10) so most features come "for free". |
| P-26 | TypeDB, HelixDB sources | 3 | medium | 26 | Driver maturity risk (Helix); TypeDB's type system needs richer schema mapping. |
| P-27 | 3D graph | 3 | low | 27 | Mostly camera/UX once P-22 exists. |
| P-28 | Browser-side WASM extensions | 5 | **very high** | 28 | Least mature tooling; Worker + component layer or `jco` transpile. |
| P-29 | Rust-native backends (terminal → code → rich text) | 5 | research | 29 | Replaces the JS packages; unlocks `dioxus-native` desktop. |
| P-30 | Collaborative editing (CRDT over the op stream) | 5 | research | 30 | [[ADR-0009 Patches not snapshots]] keeps the door open. |
| P-31 | Structured Typst / Excalidraw-in-flow | 4 | research | 31 | |
| P-32 | Git integration: status/diff decorations, commit/log, history-as-graph (`vcs-git`, `gix`) | 3 | medium | after P-06 | Folder sources are git repos in practice; server-side on web. [[Version Management]] |
| P-35 | Windows / macOS / iOS builds: verify `dx serve`/`dx bundle` on each, custom title bar on WebView2/WKWebView (macOS traffic lights vs our controls), `rfd` native dialogs, xdg-portal → OS dialogs, Vulkan/Metal for the graph view | 3 | medium | when a device exists | Design must not preclude; no work scheduled. [[Platform Matrix]] |
| P-34 | Presence: awareness protocol (cursors, selections, who's here), server websocket hub per workspace, desktop+web in one session | 3 | medium | Phase 3 | Extends the shipped session bus. [[Collaboration]] |
| P-33 | Entity log: append-only Add/Remove/SetProps/Content events, fold, snapshots, tombstones, checkpoints ↔ commits | 4 | **high** | before P-06 emits derived nodes | Shape must exist before the index creates nodes with no history. [[ADR-0012 Two histories]] |

## Reading the table
- Top-risk items are **P-01, P-05, P-07, P-16, P-20, P-22, P-28**. They are spaced across phases so that each is attempted with the most information available.
- Low-difficulty/low-risk items (P-02, P-08, P-09, P-12, P-13, P-15, P-21) are scheduled early where they unblock others, or used as "breathers" after risky ones.
- Anything marked *research* has no committed date.
