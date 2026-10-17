---
title: "Rust Code Editor Roadmap"
description: Milestones, implementation sequence, and verification plan for replacing Moonkale's CodeMirror code view with a pure-Rust editor.
tags: [editors, code, roadmap, testing]
---

## WP-69 — compact toolbar (2026-10-17)

The Rust toolbar shows four controls: Save, Reload, Wrap and ⋯, using SVG symbols,
hidden accessible labels and descriptive tooltips with available shortcuts. The
extension's gear sits in ⋯ so the overflow trigger also fits the four-control
budget. Indentation style and width are shared user preferences, now edited in
that gear's settings popover. Both adapters share these controls in `code-view`;
the core Code extension contributes the Rust palette routes too, so the minimal
distribution's Rust fallback keeps them. Preview, LSP tools, comments, folding and the
CodeMirror switch are in ⋯; commands remain available in the palette. Tab reaches
the disclosure and its commands, Enter or touch opens it, and Escape closes it
and returns focus to the trigger. Choosing a command closes the disclosure.

The cursor word is inline in the toolbar, hidden below 700 px, so it consumes no
source row. The path truncates on narrow screens and language/version metadata
is hidden there. `toolbar-icons.mjs` covers native icons, the four-control budget,
overflow keyboard/touch behavior, indentation settings and phone height;
`code-native.mjs` opens ⋯ before switching backends.


## M2 progress — local search/replace

Literal find/replace now supports Mod-F/Mod-H, shell editor actions, next/previous navigation, case/whole-word options and one-step replace-all undo. Unicode and CRLF flows pass in the shared production panel fixture. Language defaults, auto-indented Enter, Tab/Shift-Tab, comment toggling and structural bracket pairs are now connected to the production Rust surface and Workspace history. Fold gutter/placeholder controls, shell fold-all/unfold-all, conservative delimiter/Python folds and adjacent matching-bracket highlights are now implemented. Fold state survives remounts; hidden search/reveal targets expand. Wrap now follows shared settings with bounded horizontal rendering when off; optional spaces/tabs and indentation-width overrides persist through user settings and apply to both backends. Regex search, all-match/current highlighting, counts, invalid-pattern feedback and numbered/named capture replacement are implemented. Zero-width matches support navigation and replacement; Unicode/CRLF history is covered by unit and production browser tests. Grammar-based HTML element/comment, Markdown section/block and Julia keyword/comment folds are implemented with nested remount and search-reveal coverage. Syntax-aware Enter handles comment/string context, Python/Rust triggers, Julia keyword headers and HTML tag pairs, sharing indentation overrides and Workspace history. Leading bracket closers, Julia `end` and HTML closing tags now align with their matched opener in one undoable edit; inline typing and paste remain unchanged. Lean namespace/section/mutual scopes, declarations and nested proof bodies now fold through a dedicated layout provider that excludes comments/strings and preserves sibling boundaries. Additional language/custom-syntax folding, whole-document formatting and richer language policies remain open.

## Latest integration update — 2026-10-05

The production opt-in Rust panel now uses editor-core and a virtualized Dioxus view. Native deltas feed the revisioned Workspace contract; root-scoped typed view slots retain engine/selection/viewport across remounts. Browser tests of the integrated panel pass for edits, save/reload, undo/redo, duplicate views, Unicode selection, synthetic IME, clipboard, CRLF and reveal. A 3 MB single line rendered 23 rows and accepted an edit in 410 ms on this run. See [[Rust Code Editor Implementation]] for the current architecture and remaining gates. Earlier textarea implementation records below are historical; they no longer describe the production opt-in surface. Full parity/default cutover and integrated native-platform acceptance remain open.

This plan turns [[Code Editor Requirements]] into staged deliverables. It keeps CodeMirror as the safe default until the Rust view reaches feature and platform parity. Each milestone leaves a usable editor state; unfinished advanced integrations stay on the CodeMirror path until their Rust equivalents pass acceptance. The target is a single shared code view for the Code extension and Markdown Source mode, not two permanent editor stacks.

The plan is deliberately engine-neutral. First prove a candidate has an editor-owned caret/selection model, efficient change reporting, virtualized rendering, a usable Dioxus integration, and a path to decorations. The existing `dioxus-code-editor` textarea/highlight component is a useful grammar baseline, but it does not meet those capabilities by itself. The current M1 prototype still uses it and therefore does not pass M0. See [[Rust Code Editor M0 Spike]] for the engine comparison and provisional lead candidate. Do not treat removal of `document::eval` alone as satisfying editor-owned selection.

## Desktop measurement and highlighting update — 2026-10-15

The locked 21-case rerun confirms that both `performance.now()` and rAF are quantized to 1 ms in this GTK WebKit. The recorder now captures an idle baseline and labels the authorized calibrated-vsync fallback separately from `strictPassed` ([[ADR-0023 Native editor frame measurements]]). A calibrated pass is not proof of sub-millisecond p95 ≤16.7 ms.

A matched 90-key Rust-stage profile found whole-file highlight queries at approximately 7.5 ms p95. Rust now incrementally parses the same complete syntax tree but only re-queries affected root syntax regions and retains shifted spans elsewhere; whole-source replacement and other languages keep their established behavior. Differential tests compare edits/deletions at every scalar boundary, Unicode, strings/comments, EOF missing tokens and separated items against fresh dioxus-code queries. Highlight p95 fell to approximately 2.0 ms, with edit callback p95 approximately 2.1 ms (previously 7.7 ms). This is CPU-stage evidence, not a frame-gate pass.

The initial optimized Rust subset recorded Source/preview typing at 17 ms p95, but the complete repeat remained variable: Source 33/52 ms p95/max, preview 36/55 ms, preview wheel 37/196 ms, preview scrollbar 34/55 ms, 3 MB scrollbar 40/67 ms. Fifteen of 21 cases pass the calibrated cadence check; none establishes the original strict interval requirement. A descendant-inclusive gprofng profile shows substantial software-raster/compositing CPU time in Gallium/Pixman, without enough symbol/frame correlation to blame any one frame. Paint containment did not resolve this and was reverted; a one-thread Mesa diagnostic also failed. Reports remain in `/tmp/moonkale-desktop-fps-{corrected,windowed,final}-20261015*` and `/tmp/moonkale-fps-stage-comparison-20261015.json`.

A native Inspector wheel trace then identified redundant synchronous resize IPC: 1070 resize dispatches for 69 scroll events. Proportional runs now share geometry-only observers, suppress initial notifications already measured on mount and asynchronously forward actual dimension changes. Atomic preview geometry also omits redundant grapheme metadata while retaining canonical coordinates. A guarded post-resize complete matrix passed 20/21 calibrated cases: Source typing 17/20 ms p95/max, preview typing 19/39 ms, 3 MB typing 18/20 ms. Preview wheel retained a 211 ms frame despite 17 ms p95. This is not proof of causation or completion. The CPU preflight requires two consecutive <=10% active samples before every gesture and retains all settling samples. Report: `/tmp/moonkale-desktop-fps-settled-resize-20261015*` (before final metadata compaction).

After segment compaction, two further full-matrix attempts abort CPU-idle preflight after six and fourteen cases. The latter reproduces preview wheel 17/210 ms and also records Source typing 41/51 ms and preview scrollbar 31/54 ms; results remain variable and incomplete. Reports: `/tmp/moonkale-desktop-fps-compact{,-retry}-20261015*`. A partial full-sequence Inspector/Heap trace timed out stopping the profilers and did not correlate the 210 ms frame; captured GC peaks near 13 ms, so it does not establish GC causation.

Software GL includes CPU painting/compositing costs as well as Rust/DOM/layout; it does not establish hardware GPU performance. The latest strict release web rerun passes all 21 cases, p95 approximately 16.67 ms and max 33.33 ms (`/tmp/moonkale-fps-web-compact-20261015/editor-fps.json`). WP-02 and Daniel’s real desktop acceptance remain open. WP-03 acceptance work is recorded below; default cutover remains gated.

## Performance target — 60 FPS (CED-R22, 2026-10-14)

Daniel set the bar: the Rust code editor runs at **60 FPS**: typing, held arrows, scrolling, selection drag and caret movement, in an ordinary highlighted source file and in the 3 MB document. The gate is p95 frame interval ≤ 16.7 ms and no frame over 50 ms, on web and desktop, release build. The current implementation does not meet it. This gate applies from now on, not only at M5: each milestone's acceptance records the frame numbers, and a feature that pushes them over the bar is not done. First step: a benchmark suite (Playwright, `requestAnimationFrame` intervals during scripted typing and scrolling, both files) that prints p50/p95/max and the long frames' causes, run per release.

### Frame benchmark progress — 2026-10-14

`editor-fps.mjs` now records all seven interactions, long frames and build/browser metadata. The initial release web baseline exposed 233 ms p95 typing in the 3 MB file, preview scrolling up to 83 ms p95 and a 117 ms held-arrow stall. A full-sequence timeline found a 62 ms garbage collection pause after source-cell DOM churn. Bounded engine-owned status slices, fractional scroll offsets, batched height notifications, keyed rows and same-presentation ASCII text runs address those costs.

The isolated release web rerun passed all 21 cases (Chromium 153). The maxima across each file/mode's seven gestures were:

| Case | Highest gesture p95 | Longest frame | Result |
|---|---:|---:|---|
| 5000-line highlighted Rust, Doc preview off | 16.67 ms | 16.67 ms | web pass |
| Same Rust file, Doc preview on | 16.67 ms | 33.33 ms | web pass |
| 3 MB text, Doc preview off | 16.67 ms | 16.67 ms | web pass |

The raw release report is retained outside git. Desktop release frames are now measured on the private software-GL display, but still fail; see the 2026-10-15 update above. The native fixture also has a manual trusted-input recorder for the same corpus; it injects no input itself. This remains an open milestone gate until real desktop recordings meet both bounds. See [[ADR-0023 Native editor frame measurements]] and the E2E README for recording commands. The standalone benchmark remains outside run-all's strict list.

## Delivery shape

| Milestone | User-visible outcome | Main requirements | Gate to continue |
|---|---|---|---|
| **M0 — Engine and seam spike** | A selected Rust engine can edit and render a representative document in a small Dioxus host on desktop and web; mobile feasibility is known. | CED-R01, R05, R06, R09, R10, R12 | Written choice with measured input latency, edit deltas, cursor/selection API, viewport behavior, grammar/decorations strategy, target builds, size/build cost, and known gaps. Reject candidates that require JS eval for editor state. |
| **M1 — Editing foundation** | An opt-in Rust code surface edits a real Workspace document, shows line numbers and initial syntax highlighting, saves/reloads, and handles Rust-originated text updates. CodeMirror remains the default fallback. | CED-R01, R02, R04–R11 plus initial R12 coverage | Web acceptance flow passes; save/reload/external-update convergence works; Unicode edits round-trip; the 3 MB case stays within the existing E2E ceiling; no CodeMirror dependency is required to mount the Rust path. |
| **M2 — Everyday editing and language parity** | The Rust surface has the non-LSP editing tools people expect and the configured language set. | CED-R11–R14 | Search, replace, wrap, comment, fold, indentation, bracket behavior and language fallback pass parity flows on the shared panel. |
| **M3 — LSP parity** | Diagnostics, hover, completion, navigation, rename, code actions and references work through Moonkale's existing Rust LSP manager. | CED-R15–R18 | LSP flows pass using the same fake/dummy server where possible and rust-analyzer E2E for wire/integration behavior. Missing server never blocks text editing. |
| **M4 — Moonkale context** | Markdown Source links, presence, shared cursor/word context and workspace navigation behave like the CodeMirror surface. | CED-R19–R21 | Wiki, presence, search/trace reveal, multiple-window and external-edit flows pass; coordinate parity is verified for Unicode. |
| **M5 — Default cutover and retirement** | Rust becomes the only production code editor engine; stale CodeMirror-only switches, JS bundle and docs are removed or deliberately retained only for a separate view. | All MUST requirements; SHOULD decisions resolved | Desktop/web/Android acceptance, large-file and accessibility review, dependency/build-size review; then switch defaults and remove the old path. |

These are feature milestones, not fixed calendar estimates. M0 and the chosen engine determine the size of M1–M2. M3/M4 can be prepared in parallel only after the event, selection and position contracts from M1 are stable. Do not default-switch on partial feature parity: that would make markdown source, LSP or search worse for users while the old implementation remains available.

## Milestone detail

### M0 — Engine and seam spike

1. Assess the existing `dioxus-code-editor`, `editor-core` headless kernel, and at least one other Rust editor/view approach against the same must-haves: Rust-owned caret/selection; mouse and keyboard selection; IME/paste; local undo; minimal change batches; visible-range rendering for a large file; styling/decorations or an extension point; desktop/web target support and Android status. Record the comparison in [[Rust Code Editor M0 Spike]].
2. Prototype a Dioxus host backed by the selected Rust state engine, containing plain text, Rust and Markdown, including an emoji before the caret. Type, select with mouse, paste and undo; query selection from editor state (never the DOM selection API or `document::eval`); measure time from input to Workspace-equivalent document update.
3. Measure initial asset/binary size and build time using the repository's existing target/profile where possible. Check tree-sitter/Arborium features against `[[Core Languages]]`, native and wasm compilation, license, upstream maintenance and accessibility/input limitations.
4. Write a short decision note or add the decision to this roadmap: selected engine, rejected alternatives, adapter boundary, known compromises, and follow-up questions. If no candidate passes, stop before M1 implementation and decide whether to extend/fork a Rust editor engine or build a smaller engine around an existing text-buffer/rendering library.

**Exit criteria:** The prototype demonstrates that keystrokes, cursor positions and local edits remain Rust-owned with no editor JavaScript bundle/eval. A “textarea with syntax-painted overlay” alone fails if it cannot provide the selection, decorations, and incremental changes required by the next milestones.

**Current spike:** `editor-core` 0.5.0 is selected as the M1 integration candidate, subject to completing the input-host gate. The standalone Dioxus surface renders 40-row snapshots and maps browser scrolling to the requested viewport; the latest Chromium fixture suite scrolls to row 100, reveals an offscreen cursor, and exercises Rust-owned editing, Shift+Arrow extension/contraction followed by resumed plain cursor movement, undo/redo, pointer drag, Unicode-aware hover/click mapping and synthetic composition events. The preceding Firefox run exhausted memory, so Chromium is the browser currently verified. Chromium also verifies copy, cut and paste from forward and backward Rust-owned selections; Dioxus Desktop serializes clipboard events without native data, so its spike routes Ctrl/Cmd+C/X/V through a persistent Rust `arboard` clipboard owner, pending manual KDE/Wayland runtime acceptance. The composition check proves event plumbing, not real OS IME behavior. A 3 MB kernel check verifies a one-character change delta and bounded viewport snapshot (0.17 s debug smoke run; not a latency benchmark), and model tests cover direction-independent multiline selection, grapheme movement over emoji and combining marks, and Unicode-width mouse hit testing. The WebAssembly runtime required a small vendored clock portability patch (`std::time::Instant` → `web_time::Instant`) for undo coalescing. Native and WebAssembly compilation pass. The candidate comparison now covers `freya_edit`/`freya-code-editor` and `kode-core`; Freya is retained as a behavior reference but its UI is a separate native framework. The prototype model is component-local; M1 must move state, undo and scroll ownership to a `NodeId`-keyed document/root store and test panel remounts. Before the Rust view is user-enabled, close the real IME path, native clipboard shortcuts, wrapped-row/touch hit testing, syntax decorations, external Workspace updates and Android feasibility; see [[Rust Code Editor M0 Spike]].

### M1 — Editing foundation (concrete implementation plan)

**Goal:** Add a selectable Rust editor surface behind the current shared code-view panel. It edits the same `Workspace::Document` as CodeMirror, works for ordinary text and a small initial grammar set, and has reliable save/reload behavior. Keep CodeMirror as the default during this milestone so the new surface can be tested without taking away current LSP/wiki features.

**Included:** real editor-owned caret/selection and keyboard/pointer input; line numbers; theme integration; an initial explicit language-highlight set plus plain-text fallback; localized edit batches; undo/redo; document external-update path; Save, Reload, dirty state, focus, reveal; line wrap toggle if the selected engine supports it without blocking the vertical slice; responsive handling of a 3 MB text file.

**Deferred from M1:** LSP requests/decorations, find/replace, folding/comment commands, complete grammar coverage, wiki links, presence, and production-default switch. Those get proper independent milestones; M1 establishes the API they need.

#### Implementation sequence

1. **Lock the engine and event contract.** Complete M0. Specify edit delta units/order, cursor coordinates, stable document identity, and request/lifecycle rules in Rust types. Prefer using the current `BackendEvent` concepts (`Ready`, `Failed`, edit delta, cursor) but do not preserve the imperative CodeMirror `eval` handle just for source compatibility. Keep LSP, `Workspace`, save and wiki logic outside the low-level editor.

   **Contract sketch:** add a Rust-owned editor snapshot keyed by `NodeId` and an editor revision, plus events such as `Changed { base_revision, changes, selection }` and host commands for `ReplaceFromWorkspace`, `Focus`, `Undo`, `Redo`, `SetWrap` and `Reveal`. Each change uses explicitly named offsets/units. The host accepts a batch only for its expected revision; otherwise it resynchronizes from `Workspace::Document`. Keep this edit revision separate from the source/file version used by Save and conflict detection. Names may change with the engine, but stale edits must be detectable.
2. **Add the Rust surface at the shared seam.** Implement a Rust Dioxus component/internal editor module in `packages/code-view` or extract the existing native implementation into that crate. Have `CodeEditorPanel` select the Rust surface through a development/opt-in setting while CodeMirror remains the control/default. Avoid duplicating Save/Reload toolbar, dirty state, and document identity in a second extension.
3. **Build the edit adapter.** Convert editor-native transactions into localized changes and apply them to `Workspace::Document` safely. Reject invalid/out-of-order ranges without partially corrupting text; on divergence replace the view from the canonical document. Do not send the full text on every keystroke. External Rust text changes update the editor while retaining the selection/scroll where the edit allows.
4. **Complete the base interaction loop.** Render line numbers, current theme, selection/caret, text input and plain-text fallback. Wire focus, Ctrl/Cmd+S, undo/redo, Save, Reload, dirty indicator and workspace reveal. Keep the panel host's lifecycle/readiness reliable on desktop WebView and browser.
5. **Add an initial grammar slice.** Support plain text and Rust first (the project's main dogfooding language); add Markdown if needed for the shared Source mode in the same vertical slice. Confirm syntax highlighting is computed by Rust. Keep language id supplied by `Node::language_hint`; a missing or failing parser must not prevent editing.
6. **Exercise real external changes.** Verify reload/revert and a Rust-side replacement (test harness/fake event at the component boundary) update the mounted view. Confirm the user can continue editing after a missing LSP or parser, since neither service is a prerequisite in M1.
7. **Measure and review.** Compare 3 MB first-keystroke-to-dirty latency with the existing `highlight.mjs` fixture; retain its 1.5 s E2E upper bound and aim for the measured current ~87 ms Rust update on the same machine. Capture profile/target, size and measurement in the decision note. Keep the new implementation opt-in until M5.

**Likely code areas** (final paths may follow the engine decision):

- `packages/code-view/src/panel.rs`: shared toolbar, lifecycle, settings and backend selection.
- `packages/code-view/src/backend/mod.rs`: stable Rust event/delta contract and conversions, or a replacement view-facing API if declarative Dioxus fits better.
- New `packages/code-view/src/backend/native.rs` or `packages/code-view/src/editor.rs`: engine adapter/component.
- `packages/code-view/src/lsp.rs`: no feature expansion in M1; preserve current LSP manager unchanged.
- `packages/editors/code-native/*`: migrate or remove duplicate panel code once shared; don't silently strand the opt-in extension or language selection setting.
- `packages/js/codemirror/*`: untouched during the pilot; remove only at M5 after no consumer remains.

#### M1 test plan

Follow the repository's testing pyramid in [[Testing Strategy]] and [[How to Write Tests]]. Put deterministic text/position logic in unit tests; use component tests for render structure; reserve Playwright for actual pointer/keyboard, browser input, persistence and large-file integration. Avoid tests that assert engine-specific CSS/classes because the implementation is meant to be replaceable.

| ID / level | Scenario | Setup and action | Required assertion |
|---|---|---|---|
| M1-U1 unit | Insert, replace, delete and adjacent edits | Apply a batch of deltas against a known old document | New text equals expected; order and pre-edit offset semantics are explicit. |
| M1-U2 unit | Invalid delta is atomic | Include out-of-range, reversed, overlapping (if unsupported), and stale-version changes | Reject/resync indication; original canonical text is unchanged. |
| M1-U3 unit | Unicode offset round-trip | Edit before/after `😀`, combining `é`, accented BMP text, tabs and newlines; probe end-of-line/end-of-file | No panic or split surrogate; resulting text and caret/LSP coordinates match the declared conversion contract. |
| M1-U4 unit | Line/column conversion | Empty text, first/last column, LF and CRLF, multiline plus non-BMP characters | Stable zero-based position round trip; clamp/reject out-of-range consistently. |
| M1-U5 unit | External change reconciliation | Apply insertion before caret and replacement intersecting selection | Selection/scroll mapping follows the documented policy; no duplicate delta/dirty event. |
| M1-C1 component | Base editor renders | Render with document title, text, language, theme and wrap props in a `VirtualDom`/SSR harness | Editor host and accessible label exist; toolbar shows path, language/version, Save/Reload and dirty state; plain-text config is valid. Do not assert CodeMirror-specific markup. |
| M1-C2 component | Missing grammar is nonfatal | Render unknown language id or parser fallback | Plain text remains editable; component does not render an error/loading trap. |
| M1-C3 component | Mount failure path | Feed a controlled failing adapter/event in the seam harness | User-visible error state; save/reload and closing the panel remain available. |
| M1-E1 Playwright | Type and save a real file | Use disposable folder and `main.rs`; open with Rust implementation, type text, Ctrl/Cmd+S | Dirty marker appears after input; disk contains the edit; dirty marker clears only after successful save. |
| M1-E2 Playwright | Reload/revert | Save known file, make in-view unsaved edit, choose Reload | View and Workspace text equal disk; document returns clean; no second edit is emitted. |
| M1-E3 Playwright | Rust-originated edit while mounted | Send a test-only Workspace document replacement (or drive an existing replace flow) while view is open | View converges to canonical text; caret is retained if before/after the changed span; view does not overwrite the external value. |
| M1-E4 Playwright | Pointer selection and clipboard/input | Click a middle position, select range, paste, undo/redo | Correct range changes; Rust document and view agree; undo/redo restore expected text. Include emoji before the selection. |
| M1-E5 Playwright | Large document | Reuse `highlight.mjs`'s 3 MB fixture; type one character at start | Dirty state reaches the shell within 1.5 s (existing suite ceiling); no whole-document event per key; record latency for comparison. |
| M1-MAN manual smoke | Native input | Desktop WebView: IME/composition, dead key, clipboard, focus switching and reopen; Android soft keyboard if engine claims mobile support | Text/caret behavior remains usable; no JS editor runtime/eval is needed; note target-specific failures. |

M1 tests that can run in ordinary CI should avoid external language servers, display servers, and network access. The E2E large-file test should use the fixture and run on the normal web E2E path; a short manual native-input checklist is needed because Playwright does not prove OS IME behavior.

#### M1 implementation record (2026-10-04)

The first opt-in vertical slice is implemented in `packages/code-view/src/native_panel.rs`; the CodeMirror surface remains the default. `packages/editors/code-native` is now a thin extension adapter to the shared component, and Markdown Source mode selects the Rust panel when `Workspace::editor_for(node)` is `native`. Rich Markdown mode is unchanged. See [[Rust Code Editor Implementation]] for code-level details and known gaps.

**Implemented and verified:** deterministic byte-range `TextEdit` generation/application and local undo/redo history; stale-range rejection; Unicode-safe UTF-8 slicing boundaries; Rust/Lean/Nix/Julia plus enabled grammar selection and unknown-language plain-text fallback; line numbers and theme selection; Workspace text update, dirty state, Save/Reload, shell Save/Undo/Redo/Close and editor switching; opt-in wiring for generic text and Markdown Source; active selection anchor/head reporting from both editor paths to Workspace; and UTF-16-aware `cursor_word()` after a non-BMP prefix. Unit tests cover edit/history, UTF-16 offset conversion and word lookup; the Playwright flow covers the Rust editor selection and CodeMirror selection reports as well as syntax, save/reload and switching.

**M1 prototype is not an accepted engine:** the selected `dioxus-code-editor` component is a controlled textarea with syntax-painted overlay, not a full editor core. Dioxus delivers the new full `String`; this adapter computes one minimal replacement in Rust, so input delivery and diffing remain O(document length). Both editor paths report selection anchor/head, but the Rust path uses Dioxus eval and browser owns the state. This fails M0's editor-owned-state gate. External canonical text changes replace the displayed value and clear Workspace selection; caret/selection retention is not guaranteed. Virtualized rendering is not established. The 3 MB file met the 1.5 s E2E latency ceiling (870 ms on this run), but that does not establish virtualization or sustained typing performance. Clipboard, desktop WebView interaction and Android IME still need platform verification. Keep it opt-in while M0 is completed; do not expand M1 on this engine.

**Test status:** `cargo test -p moonkale-code-view -p moonkale-ext-api -p moonkale-editor-code-native -p moonkale-editor-markdown` passes (10 code-view tests, 20 ext-api unit tests plus integration/doc tests, and the Markdown test); `cargo check -p desktop --features desktop --bin moonkale`, CodeMirror `npm run build` and `npm run check` pass. The Playwright suite passed on 2026-10-04 against a Dioxus web server and disposable fixture: syntax tokens, edit/save, Unicode selected-range replacement and undo/redo, exact reload, backward-selection and keyboard selection reporting with an emoji prefix, selection ranges reported by both editor paths, selection clearing on reload and editor switch, non-transparent computed caret color, and 3 MB input-to-dirty in 870 ms. Unit coverage verifies UTF-16 offset conversion and word lookup after non-BMP text. Caret retention after external updates, native clipboard/IME, and component-level selection harness remain unverified.

#### M1 contract and Workspace ownership (2026-10-05)

`moonkale-ext-api::editor` now defines `EditorSnapshot`, a distinct editor revision, atomic localized UTF-8 byte-range `EditBatch` changes, UTF-16 selection/reveal coordinates, host commands, editor events, and stale/invalid rejection with a canonical snapshot. `Workspace::DocsState` owns each `RevisionedDocument` and its bounded forward/inverse undo transactions in a root-scoped map keyed by `NodeId`. `RustCodeEditorPanel` now submits its localized input diff as a batch, commits only against the matching Workspace base text, resyncs after rejection or divergence, and ignores late caret reports from older revisions. Reload and other external replacements invalidate the session history and selection. This contract step is connected through the existing textarea adapter; wiring `editor-core` native deltas and its caret/selection model remains future work. `cargo check -p moonkale-code-view --lib -p moonkale-ext-api` passes. No tests were added or run for this slice.

### M2 — Everyday editing and language parity

1. Complete current CodeMirror language map, then add the missing [[Core Languages]] where the selected Rust grammar ecosystem supports them. Keep a single language-hint registry for highlighting and per-language optional metadata (comment syntax, indent, folds); do not conflate grammars with LSP server discovery.
2. Implement bracket matching/closing, indent-on-input, toggle comments, fold gutter, fold/unfold all, selection match highlighting, soft wrap, and live theme update. Unknown languages remain plain text and no parser failure disables edits.
3. Implement editor-local find/replace with Mod-F, Mod-H, next/previous, selection matching, and `EditorAction` entry points. All replacements use normal deltas to Workspace; do not implement workspace-wide search in this component.
4. Verify keyboard and mouse paths, macOS Cmd vs other Ctrl primary modifiers, accessibility labels, paste/IME, long lines and large files.

**Exit:** code editor editing behavior is equivalent for configured Moonkale actions; tests cover each grammar capability as supported and explicit fallback for unsupported language operations.

### M3 — Language Server Protocol

**Diagnostics slice (2026-10-06):** document synchronization and diagnostics now run through the shared manager in Rust views. Gutter markers, underlines and clickable messages are connected; versioned stale reports and empty publications are handled. Deterministic production-panel browser coverage passes for Unicode/CRLF positions and duplicate/remount lifecycle. Hover is now implemented with debounce, cancellation, UTF-16 coordinates, keyboard activation and stale-result handling. Completion now supports async suggestions, keyboard/mouse acceptance, stale-result cancellation and atomic text/additional edits. F12 definition now reveals same-file/folded targets and opens cross-file targets in the originating source, with stale-request and post-load guards. F2 rename now has a focused prompt, guarded multi-file staging, per-document undo, unsaved closed-file targets and LSP synchronization. Mod-. code actions now include diagnostic context, preferred/disabled entries, lazy resolve and guarded multi-file edits. Shift-F12 references list and reveal deduplicated same-file/folded/cross-file results. Live browser/native UI integration is now covered by the WP-03 acceptance record below; command-only server actions remain outside the shared client's supported feature set. See [[Rust Code Editor Implementation]].

1. Preserve `LspManager` ownership/session lifecycle and the current per-language/per-folder sharing model. The editor requests work with stable IDs and zero-based line/UTF-16-column positions; the panel translates results only once.
2. Add incoming diagnostic marks/gutter and readable message affordance; replace/clear marks when diagnostics update.
3. Add hover request/result; asynchronous completion with item labels, kind, detail, insertion text, cancellation/stale-response behavior, keyboard accept/dismiss and mouse selection.
4. Connect F12 definition (same-file caret vs cross-file open/reveal), F2 rename prompt, Mod-. code actions and Shift-F12 references to existing Rust `LspSession`/`apply_workspace_edit` flows.
5. Keep editing fully available if no LSP is configured, fails to spawn, closes, or takes too long. Errors belong in status/panel feedback and must not freeze input.

**Exit:** reuse the current `lsp.mjs` and `lsp2.mjs` behavior scenarios with engine-neutral selectors; add a deterministic fake LSP session test for request positions, stale replies and returned edits.

### M4 — Moonkale context and shared workspace

1. Feed `Workspace::cursor`/`cursor_word()` from the engine's actual caret (not from guessed key events); use a throttle comparable to the current 250 ms report. Validate UTF-16 vs internal position conversion.
2. Render presence marks by line and user initials; update on presence changes and document identity changes without remounting the editor. **Implemented:** native gutter subscribes to workspace presence, excludes this window and other files, and shows initials once per logical line with member names in the tooltip. Dedicated gutter space preserves line numbers, diagnostics and folding controls.
3. Add Markdown Source wiki spans, resolved/unresolved styling, async `[[` candidate lookup, no duplicate close brackets, modifier-click following, and refresh when text/index epoch changes. **Implemented:** shared Rust lookup APIs drive native marks and completion; guarded follow/create preserves source identity and rejects stale navigation. Unicode/CRLF, index refresh, atomic completion undo and delayed-reply browser checks cover the native Markdown surface.
4. Support shell reveal from search, trace, definition and other callers; selection and scroll are placed correctly. Exercise agent/reload/workspace replacement while open, multiple document tabs/windows, and panel moves/remounts. **Browser integration covered:** workspace API reveals use canonical text and UTF-16 coordinates; activation restores retained caret/word context; replacement, reload, dock moves and close/reopen ordering have deterministic coverage. Two browser windows exercise real session offers/acknowledgements. Only saved tabs are offered; unsaved edits remain in the origin even when added after an offer. Native OS gestures and platform acceptance remain gates; live-server UI evidence is recorded below.

**Exit:** current wiki, presence and reveal workflows pass on Rust editor, including non-BMP text preceding a wiki span or cursor.

### M5 — Default cutover and retirement

1. Run the full requirements matrix on Linux desktop, web and Android; explicitly record macOS/Windows limits where the repo does not test them. Validate IME, copy/paste, accessibility, theme changes and pointer selection on native targets. **Partial acceptance (2026-10-07):** live rust-analyzer protocol features and native XWayland typing/keyboard selection/forward-backward pointer dragging/Unicode clipboard/history/resized input pass. Cursor-word layout stays stable during selection. A default Dioxus native DOM/painting probe also passes without external renderer overrides; the current owned-window OS retry remains blocked by mapping. Combined native UI/live-server, default-configuration OS input, IME, accessibility and other platforms remain gates; see [[Rust Code Editor Native Acceptance]].
2. Benchmark representative small, 3 MB and large-source files; record typing latency, memory/rendering behavior, release binary size and clean/incremental build cost. Compare against the existing baseline under the same target/profile.
3. Make the Rust implementation default only after M1–M4 exit criteria pass. Migrate the stored implementation setting (`codemirror`/`native`) and per-document choices safely; decide whether to keep one fallback release or remove the legacy choice.
4. Remove CodeMirror-only backend code, `packages/js/codemirror`, generated bundle, dependencies, switch UI and obsolete language notes once there are no consumers. Keep Milkdown/xterm and other JS views untouched.
5. Update [[Code Editor]], [[Code Editor Implementations]], [[Code Editor Requirements]], [[Status]], crate implementation notes and E2E docs to reflect what is actually built; delete obsolete claims only after verifying call sites.

**Exit:** no default or Markdown Source path loads CodeMirror; CI/build has no code-editor JS package; docs and settings agree with shipped behavior.

## Cross-milestone risks and decisions

View-layer preparation for ADR-0017 is tracked in [[Rust Code Editor Live Preview Plan]]. It preserves the uniform code-layout fast path while planning variable row heights, proportional geometry and shared decorations. Markdown live preview itself is deferred.

The uniform geometry boundary and variable-height index are implemented. A feature-gated host fixture verifies taller text rows, anchored blocks, scrolling, reveal, remount, resize and folding. Fixture widget measurements reject stale results, and retained source anchors support view-only rewrap and fractional remount restoration. Search, diagnostics and wiki marks now use a shared revision-bound decoration interface. A bounded proportional geometry fixture now passes browser and native WebKit DOM/layout checks through one Range adapter. A bounded, feature-gated proportional run now uses virtualized source rows, decorated spans and existing Workspace history. Measured Up/Down and Shift selection now retain a pixel column within the bounded line. Visual Home/End with wrap affinity and pixel-column transitions to ordinary rows are implemented in the opt-in fixture. Font generations, delayed/failed/preloaded font changes and resize/caret stability are verified in browser and native DOM fixtures. Whole-document pixel wrapping and real OS/IME acceptance remain; replacement/widget rendering and markdown providers remain deferred.

- **The engine choice is the critical path.** A textarea offers input/IME but does not itself expose an editor-owned cursor/selection, virtualized lines, folding or decorations. A richer editor core may have less Dioxus integration or a larger binary. Measure instead of selecting from crate descriptions alone.
- **Coordinates are easy to get subtly wrong.** CodeMirror offsets are UTF-16; Rust strings are UTF-8; Unicode scalar/grapheme cursor movement is a third concern. Keep separate types/names and test conversions before LSP and wiki integration.
- **External updates compete with local history.** Define whether an incoming edit maps selection and becomes an undo boundary. Agent/LSP replacements must not be echoed as unrelated user edits or overwrite newer local edits.
- **Feature and language lists are not the same thing.** Syntax grammar presence does not guarantee LSP, folding, indentation or comment support. Represent each capability independently and communicate plain-text fallback clearly.
- **Test selectors must be engine-neutral.** Existing E2E scripts inspect `.cm-*` elements. M1 should introduce stable semantic hooks (for example `data-editor`, accessibility roles and document labels); migrate shared workflows before deleting the old DOM.
- **Web success is not native input parity.** Validate WebView composition, clipboard and Android soft keyboard before making the Rust view the only default.

## Test portfolio by milestone

| Milestone | Unit/component | Integration/E2E | Manual/platform |
|---|---|---|---|
| M0 | Offset/delta prototype checks; no permanent engine tests until selected | Render + edit prototype on web and desktop | Input, IME, selection and scroll on desktop; Android feasibility |
| M1 | Delta application, coordinate conversions, failure/fallback and render structure | Save/reload, external update, input, 3 MB responsive typing | Desktop WebView IME/clipboard; Android input smoke |
| M2 | Search query/replace range logic, language capability registry, fold/comment metadata | Search/replace, wrap setting, grammar matrix and fallback | Keyboard layout/modifier check on Apple if available |
| M3 | Position translation, diagnostic replacement, request-id/stale-result behavior | Existing LSP + refactor flows with rust-analyzer; fake server for deterministic cases | LSP startup failure and slow server behavior |
| M4 | Wiki span remapping, candidate response correlation, presence formatting | Wiki, presence, reveal, multi-window, agent/external edit | Cross-window drag/reveal if supported |
| M5 | No-JS/dependency boundary and settings migration | Full web E2E suite, release builds and target checks | Desktop and Android acceptance; accessibility/input review |

## Related decisions

This plan depends on the replacement requirements in [[Code Editor Requirements]], current implementation notes in `packages/code-view` and `packages/editors/code-native`, the layer/testing guidance in [[Project Structure]] and [[Testing Strategy]], and the documented language set in [[Core Languages]]. Do not update the general product milestone numbering: this is a **code editor sub-roadmap**.


### Bounded private-cache follow-up — 2026-10-15

With master `0aef7472` (private persistent cache/data/config homes), the release fixture was rebuilt. One three-case wheel warm-up attempt and one full 21-case guarded attempt both aborted before launching a case: aggregate CPU never met two consecutive <=10% samples in six attempts. No cache-warm comparison or new frame result is available; neither attempt proves or disproves removal of the prior210ms preview-wheel pause. Raw preflight samples and logs: `/tmp/moonkale-next3-{warmup,desktop}*`. No further headless gate iteration is planned in this run. The real GPU/desktop check remains Daniel's final acceptance; its commands, files, seven gestures and strict versus calibrated criteria are in the coordinator handoff. WP-03 proceeds through its own milestone gates while this desktop acceptance remains explicitly open.


### WP-03 — M3 integration acceptance (2026-10-15)

The existing M3 implementation now has a shared live-server UI gate rather than
separate protocol-only evidence. `lsp.mjs` and `lsp2.mjs` select Rust with
`LSP_EDITOR=native` and CodeMirror with `LSP_EDITOR=codemirror`, using the same
scenarios and an engine adapter. Hover and F12 websocket requests are checked
against canonical UTF-16 positions after an emoji/CJK comment in CRLF source.
Diagnostics/save-to-clear, completion, unsaved rename, stale-prompt rejection,
reference reveal and lazy edit-based code actions are exercised through the UI.

The desktop fixture also has an explicit live-server mode and an owned-display
runner, `native-editor-live-lsp.py`. Native GTK WebKit passes diagnostics, hover,
F12, F2 rename, Shift-F12 references, extract-variable code action and completion
against rust-analyzer 1.98.1. Its report retains original/final CRLF text and user
agent; disk bytes stay unchanged. This closes the earlier **native UI + live LSP**
integration gap. It uses DOM events, so physical input/IME acceptance remains a
separate check. No production editor behavior or default changes in this step.

Command-only server actions remain unsupported by both implementations. The shared
client filters them; declared edits and lazy edit resolution retain existing
behavior, without executing `workspace/executeCommand`. This is the current parity
boundary, not a new permission to execute commands. Server-command support is a
separate feature, not a reason to bypass the M5 gates.

The latest release web frame evidence remains the unchanged 21/21 strict matrix
above (p95 about 16.67 ms, max 33.33 ms); this test-only work does not change the
production render/input path. Daniel's physical-desktop frame check remains open,
so functional LSP acceptance does not close CED-R22 or authorize default cutover.

### WP-03 — remaining default-cutover gates

| Gate | Evidence / required next action |
| --- | --- |
| M3 functional parity | Shared fake-server regressions, real-server protocol and browser/native UI checks; exact commands in the E2E README. |
| M4 context | Existing Rust wiki, presence, Unicode caret/word, reveal/replacement/remount and real two-browser-window scenarios; retain their native OS gesture limitation. |
| CED-R22 desktop | Daniel's physical GPU/session 21-case check, including the preview-wheel pause. Do not repeat WP-02 headless profiling. |
| Native input and accessibility | Earlier Linux OS/IME evidence is in [[Rust Code Editor Native Acceptance]]. Chromium keyboard/AX and native AT-SPI semantics now pass; bounded-context and spoken-reader/platform limits are in [[Rust Code Editor Accessibility and Cutover]]. |
| Android | Integrated editing, IME, clipboard, selection, source/wiki and LSP availability smoke remain unaccepted. No device run in this step. |
| M5 size/build/dependencies | Reproducible build/size evidence, tested one-setting migration and the conditional retirement plan are in [[Rust Code Editor Accessibility and Cutover]]. Default flip remains gated. |

Rust remains opt-in. Keep CodeMirror and its settings/bundle until M5; do not treat
partial platform evidence as permission to remove the fallback. macOS and Windows
remain explicitly unverified here. P-198 records this acceptance work; ADR-0029 remains unused.


WP-03 regression verification: all eleven selected Chromium production-panel
suites pass: `native-editor-{diagnostics,hover,completion,definition,rename,tools,
wiki,presence,integration,windows,workspace}.mjs`. The M4 subset covers Unicode
caret/word context, presence identity/lifecycle, wiki stale replies, reveal,
external replacement/reload, dock remount and actual two-browser-window offers
without moving unsaved buffers. Logs: `target/wp03-e2e/{suite}.log` and
`/tmp/moonkale-wp03-fake-checks.log`. These are browser context gates, not native
window-manager gesture or cross-platform acceptance. All AGENTS.md repository
gates pass (671 tests including doctests); desktop fixture Clippy/build also pass.

The final shared live browser matrix passes all four runs: `lsp` and `lsp2` on
Rust and on the CodeMirror control (Firefox, unchanged release web host). Both
save flows verify exact CRLF bytes. Matrix log: `/tmp/moonkale-wp03-lsp.log`.


### NEXT-4 — accessibility and prepared cutover (2026-10-15)

[[Rust Code Editor Accessibility and Cutover]] is the M5 acceptance record and
conditional retirement plan. The Rust surface now has one file-named editable
input, bounded canonical live caret/selection context, keyboard Tab-focus mode,
keyboard link/image following and hover focus return, visible token-based focus and
reduced-motion rules. Actual Chromium AX and native GTK WebKit AT-SPI tree checks
pass; this is not a spoken-reader transcript or a full editable-text document API.
The sampled built-in source contrast minima are 5.54:1 light and 4.58:1 dark.

The existing `editor.implementation` setting now suffices to activate Rust unless
explicitly disabled. Resolution preserves raw settings; explicit per-document
fallback stays effective. Full-app migration verifies unsaved buffers, active
canonical UTF-16 selection, exact CRLF/Unicode saves and restored saved tabs.
`settings::DEFAULT_CODE_EDITOR` centralizes the future default, including minimal
distributions and native adapter activation. It remains **codemirror**, with a
deliberate guard test. No CodeMirror consumer, asset or fallback is removed.

The latest NEXT-4 strict release-web matrix is 20/21: preview-on scrollbar
dragging measured p95/max33.335ms. Keep this failure distinct from the earlier
21/21 WP-02 run. No new physical-desktop acceptance is claimed.

NEXT-5 reproduces that failure, isolates the ancestor focus outline with a
Chromium profile and same-build CSS comparison, and replaces it with a reserved
transparent border whose color changes on focus. The rebuilt strict release-web
matrix passes **21/21**: every p95 <=16.670 ms; largest frame 33.335 ms; preview
scrollbar p95/max 16.670/33.335 ms. Raw
`target/next5-fps-final/editor-fps.json`; Chromium153.0.8010.12, bench lock and both
heavy slots held, bounded quiet-CPU preflight recorded. First-drag resets are
preserved. Keyboard/native focus checks verify the border, and no accessibility
context or selection restoration is removed. Desktop/Android gates stay open.

Build/size measurements, complete test commands, remaining device/frame checks
and the staged zero-consumer retirement audit live in the acceptance record.
Daniel's physical-desktop CED-R22 matrix and Android acceptance remain required
before flipping the default. macOS/Windows remain explicitly unverified.
