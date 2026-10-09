---
title: "Rust Code Editor M0 Engine Spike"
description: Candidate assessment for moving caret, selection and edit state out of the browser textarea.
tags: [editors, code, architecture, spike]
---

## Latest integration update — 2026-10-05

The production opt-in Rust panel now uses editor-core and a virtualized Dioxus view. Native deltas feed the revisioned Workspace contract; root-scoped typed view slots retain engine/selection/viewport across remounts. Browser tests of the integrated panel pass for edits, save/reload, undo/redo, duplicate views, Unicode selection, synthetic IME, clipboard, CRLF and reveal. A 3 MB single line rendered 23 rows and accepted an edit in 410 ms on this run. See [[Rust Code Editor Implementation]] for the current architecture and remaining gates. Earlier textarea implementation records below are historical; they no longer describe the production opt-in surface. Full parity/default cutover and integrated native-platform acceptance remain open.


# M0 status: editor-core selected for M1 integration; input/platform gates remain open

The current `dioxus-code-editor` widget does not meet the core requirement. It renders a controlled `<textarea>` and sends the complete text to Rust through `oninput`; browser input state owns the caret and selection. Adding more event callbacks or replacing `document::eval` with `web_sys` reads would remove eval but would still leave selection owned by the browser widget. That is not an acceptable completion of M0.

## Candidate comparison

| Candidate | State and rendering model | Fit for Moonkale | M0 assessment |
|---|---|---|---|
| `dioxus-code-editor` 0.1.2 (current) | Dioxus component with a controlled textarea and a separate Rust syntax-highlighted layer. Public API reports full text only. | Good Rust grammar/theme baseline and basic typing; no editor-owned selection, incremental public edits, or virtualized lines. | **Reject as the editor core.** Keep its `dioxus-code` highlighting code only if it can be cleanly separated from the textarea widget. |
| `editor-core` 0.5.0 | Headless Rust kernel: piece-table text buffer, cursor/selection commands, undo, visual-row/layout state, viewport snapshots and change notifications. UI is intentionally left to the host. | Its one-buffer/one-view API maps to one Workspace document. Dioxus can render only visible grid rows; Rust input handlers can issue commands and read cursor/selection from the kernel. Positions are character offsets, so adapters must explicitly convert to Moonkale/LSP UTF-16. | **Selected as the M1 integration candidate, subject to IME/platform gate.** It passes the core Rust-owned state, viewport snapshot, Unicode edit-delta, desktop-build and wasm-build checks. Do not cut over until input gaps below are closed. |
| `ratatui-code-editor` 0.0.6 | Tree-sitter + rope-backed terminal widget; includes editor selection and editing behavior. | Its Ratatui rendering/input abstractions do not plug into Dioxus/WebView. Porting/adapting them would still mean building our own frontend. | **Reject as a direct widget.** Its feature list may inform the behavior matrix only. |
| `freya-code-editor` 0.4.x / `freya_edit` 0.4.x | Rust UI plus a separate Rope-backed editing model, Tree-sitter language queries, virtual scrolling, pointer selection, and `PreeditState` for IME composition. | Strong independent interaction reference; it is a complete native UI in Freya's own scene graph. Freya 0.4 no longer uses Dioxus, so embedding its view in Moonkale would mean adopting another UI stack. Its platform and WebAssembly fit are not established here. | **Reject as a direct component; retain as the best behavior/API reference.** Compare its composition lifecycle, clipboard and virtual-scroll behavior when building the Dioxus input adapter. |
| `kode-core` 0.2.0 | Small Rust model with Ropey buffer, cursor/selection, edit transactions and undo history. | A clean buffer/state alternative, but its documented surface does not provide a Dioxus view, viewport snapshots, wrapping, pointer mapping, clipboard/IME host, or the broader editor command/layout layer that `editor-core` already supplies. | **Reject as a replacement candidate for this milestone.** It would leave Moonkale building more editor infrastructure without resolving the input-host question. |
| `taino-edit-dioxus` 0.7.0 | Rust editor state and transforms with a Dioxus adapter; adapter mounts a DOM editor view and wires input, composition, paste, pointer and selection via Rust `web_sys` listeners. | Strong reference for an editor state/transaction seam, but its document/schema model is for structured rich text, not a code buffer. It still uses DOM as the display/input surface, though it does not use `document::eval`. | **Do not adopt as the code engine.** Revisit its adapter design only if our prototype needs a concrete DOM-event integration pattern. |

## Provisional architecture

Use a Rust-owned editor state as the source of truth and a Dioxus renderer as a projection of its visible rows. Start with `editor-core::EditorStateManager` for a single document/view. Handle text, movement, selection, undo/redo and viewport changes by issuing typed Rust commands. Translate only accepted text deltas into Workspace changes. Render the current selection/caret from queried Rust state; never query DOM selection to decide the editor selection. Keep the current CodeMirror route untouched as fallback.

The browser/WebView still has to deliver physical key, pointer and composition events. A Rust Dioxus host can receive ordinary Dioxus events; if a target requires low-level platform APIs, those APIs may deliver input but must not become the source of truth for editor selection. An offscreen input element may be considered only for IME text composition, and only if composition updates are converted to editor commands while caret/selection remain in the Rust model.

## Spike gate before M1 work continues

1. Build a small Dioxus surface backed by editor-core for plain text, Rust and Markdown. Render visible rows from the viewport snapshot rather than one element per document line.
2. Demonstrate keyboard movement, shift-selection, pointer click/drag, selected-text replacement, undo/redo, Unicode grapheme movement, copy/paste, and composition input. The cursor and selection assertions must query editor-core state, not the DOM selection API.
3. Feed resulting deltas to an in-memory Workspace-shaped document and verify their old-document character offsets convert safely to/from UTF-16 for Moonkale and LSP.
4. Build for desktop and `wasm32-unknown-unknown`, then run the interactive host on desktop and browser. Measure 3 MB input-to-document-update and visible-row count. Confirm viewport size and retained allocations do not grow linearly with the total number of rendered lines.
5. Verify license and dependency/build impact before committing to editor-core. Record gaps and make the engine decision in this file.

## Prototype progress

The published `editor-core` 0.5.0 API provides `EditorStateManager`, typed cursor/edit commands, selection state, viewport snapshots and a piece-table buffer; its declared minimum Rust version is 1.91. The first registry request briefly failed DNS resolution for `index.crates.io`; retry succeeded. The API spike found that its public `EditorStateManager` delegates text access to its inner editor, which was corrected.

The current Dioxus spike is in `packages/code-view/src/editor_core_spike.rs`, with a standalone web host at `packages/web/tests/fixtures/editor-core-spike`. It renders a 40-row requested snapshot in a scroll-sized spacer and updates the requested first row from Dioxus scroll events; the browser test scrolls to row 100, checks cursor-driven reveal, and confirms the rendered range follows. It also handles character input, cursor movement, keyboard selection, undo/redo, mouse placement and drag selection by issuing Rust commands. Pointer placement and hover-position reporting use the pointer's half of a rendered cell to map before/after its scalar; cell widths and hit-test tests follow `editor_core::char_width`, including two-cell CJK/emoji and zero-width combining marks. The browser regression checks both sides of the emoji cell and clears the reported position when the pointer exits the viewport. Repeated Shift+Arrow extension/contraction, cursor movement after a selection collapses to empty, undo, pointer drag, grapheme cursor movement, composition start/update/end dispatch, committed composition text, and ordinary input through the offscreen textarea sink pass in the Chromium fixture suite. A Chromium clipboard check verifies copy, cut and paste against the browser clipboard using the Rust-owned selection, including backward selection. Copy/cut use the native browser clipboard event's `text/plain` data and cut edits only after the clipboard write succeeds. Dioxus Desktop serializes clipboard events without native clipboard data; the desktop spike handles Ctrl/Cmd+C, X and V directly through a long-lived Rust `arboard` clipboard owner. A temporary Rust probe verified Wayland clipboard set/get in the active KDE session and restored the previous text; the spike's Ctrl/Cmd shortcuts still need direct UI acceptance. User feedback caught caret rendering issues: a duplicate caret and a visible glyph that shifted text as the cursor moved. The caret now uses one row-level render path with a zero-width inline anchor and a 1px CSS bar at the anchor's left edge; the browser test asserts the bar shares the first character cell's x-position and remains thin, and verifies moving the caret leaves document character positions unchanged. `cargo test -p moonkale-code-view --lib` passes 19 unit tests, including Unicode-width hit testing, direction-independent multiline selected text, repeated grapheme selection, and 3 MB bounded-snapshot/edit-delta checks. The standalone example compiles for the Dioxus desktop renderer and for wasm with its `spike-web` feature.

The first browser interaction exposed a runtime incompatibility: upstream `editor-core` calls `std::time::Instant::now()` for undo coalescing, which panics on this `wasm32-unknown-unknown` runtime. Moonkale carries a small local source fork under `packages/code-view/vendor/editor-core` that substitutes `web_time::Instant`; this keeps the state engine in Rust and uses the platform clock only for undo timing. This fork is a dependency/build-size and maintenance cost that must be included in the engine decision.

The prototype remains incomplete. Its composition browser test dispatches synthetic composition events; desktop KDE Pinyin now also passes manual input acceptance. Browser clipboard copy/cut/paste are covered with Chromium, including backward selections; desktop Ctrl+X/C/V now passes manual KDE acceptance through the Rust `arboard` path because Dioxus Desktop does not expose clipboard data through its serialized event. The Chromium fixture verifies Shift+Arrow extension/contraction in both directions, clears empty selections in the engine, and confirms plain cursor navigation resumes immediately. Unicode-width hit testing is covered in the unit suite; browser checks both sides of the emoji cell, pointer hover-position mapping, drag selection, and keyboard selection at deliberate character boundaries. Word/line snapping and touch hit testing remain unimplemented. Scrolling updates the visible-range snapshot, and arrow-key movement reveals a cursor that leaves the viewport; pixel-precise smooth scrolling and wrapped-row mapping are not implemented. Syntax decorations and external Workspace updates remain unverified. The spike's model is component-local and is discarded on unmount; this is acceptable for the standalone prototype, but M1 state, undo history and scroll position must live in a document/root-scoped owner keyed by `NodeId`, as described in the handoff. Add an explicit panel-remount test before integration. Keep these items as explicit integration gates; M0 selects the engine for M1 implementation, not for production cutover.

### Desktop selection follow-up (2026-10-05)

The user reported that selecting text in the desktop spike was not possible and that caret placement advanced by a character. Inspection found that selected ranges had no visible highlight, and pointer placement issued `MoveTo` without first clearing an existing selection; `editor-core` preserves its selection across `MoveTo`, making a subsequent click appear ineffective. The spike now renders selected cells with a visible background, clears the existing selection before mouse placement, and extends drag selection on `onmousemove` so movement inside one wide character can select it. Added regressions cover visible emoji highlighting, same-cell emoji drag, click after selection, and selection clearing. All 17 unit tests, desktop and wasm checks, and the Firefox interaction suite pass. The updated native spike is open for KDE manual acceptance; the clipboard library's Wayland set/get probe passed with the previous text restored. The Ctrl/Cmd clipboard shortcuts and OS IME still need direct desktop interaction checks.

### Selection navigation follow-up (2026-10-05)

After the user confirmed that selection highlighted, they reported unreliable highlighting and a caret that eventually stopped moving. Inspection found two more state issues: the selection renderer treated a backward selection as an empty range, and unshifted arrow keys did not collapse an active selection before moving. The renderer now compares ordered endpoints; arrow keys collapse to the nearest relevant edge and clear selection, while Shift+Arrow continues from the active end. Pointer drag tracking also checks that the primary button is still held, so a missed mouse-up cannot leave drag mode active. At this stage, the unit suite had 18 tests and desktop/wasm checks passed. The Firefox rerun reported `NS_ERROR_OUT_OF_MEMORY`; the later Chromium interaction suite verifies the updated web flow.

The user then observed that the open desktop spike hung after starting a Shift+Arrow selection. Repeated extension previously cleared the selection, moved to its active endpoint, moved one grapheme, and rebuilt the selection for every key event. It now preserves the anchor and existing selection while moving, then updates only the active end. A regression extends and contracts 100 characters and confirms the selection clears at the anchor. The suite now has 19 passing tests; desktop and wasm checks pass. The user confirmed repeated Shift+Arrow interaction works well in the desktop spike.

### Pointer hover mapping follow-up (2026-10-05)

CED-R15 needs the editor to translate an LSP hover pointer position; it does not require a visual background for the hovered character. The spike now tracks the mapped `(line, column)` under the pointer using the same Unicode-cell midpoint logic as click placement, exposes it as a test-visible data attribute, and clears it when the pointer leaves the viewport. The main Chromium fixture suite checks both sides of the emoji cell and passes the full interaction flow. The clipboard fixture also passes copy/cut/paste; its backward-drag coordinates were corrected to end before the first cell so the test selects `fn` rather than `n `.

The Shift+Arrow browser sequence caught the precise stuck-cursor condition: collapsing a selection to its anchor left an empty `Selection` inside `editor-core`. The handler now issues `ClearSelection` when the active endpoint reaches the anchor. The refreshed Chromium suite confirms that an unmodified ArrowRight then advances the cursor. Desktop and wasm builds and all 19 unit tests pass. The latest native spike, including pointer hover mapping and the empty-selection fix, is open for manual input acceptance.

The initial Firefox run of the expanded fixture suite exhausted memory. The main interaction test now uses Chromium, which runs successfully here; its assertions cover the virtual viewport, selection recovery, and hover mapping.

### Native input acceptance and trailing-line click fix (2026-10-05)

The user confirmed that Pinyin input through KDE and desktop Ctrl+X/C/V all work in the open spike. This completes the M0 manual IME and clipboard acceptance for this KDE/Wayland setup. The target machine is KDE Plasma on Wayland, using Fcitx5 and its Pinyin addon; after probing the input method, the original `keyboard-de` layout was restored.

The user then noticed that clicking the blank portion of a line after its last character did not move the caret to that line. The renderer only attached pointer handlers to character cells, leaving the rest of each row without a hit target. Each rendered row now has a trailing-space hit target beginning just after the gutter and text cells; clicking there moves the Rust-owned caret to that row's end. The desktop spike was rebuilt with this fix. This correction has compile validation; manual confirmation of the trailing-space click is still pending.

## Candidate decision and large-document check

The engine comparison now includes `freya-code-editor`/`freya_edit` and `kode-core` as independent Rust alternatives. Freya is the useful input behavior reference: its editor source uses a Rope model, a virtual scroll view, and an editable model that exposes IME preedit state. Freya itself is a native UI framework and its current 0.4 release no longer uses Dioxus, so adopting its visual component would replace too much of Moonkale's UI stack. `kode-core` supplies basic model primitives but would still leave Moonkale building viewport/layout and the complete event host. `ratatui-code-editor` is similarly terminal-specific. On current evidence, `editor-core` has the best reusable model and snapshot boundary for a Dioxus renderer; the local `web_time` fork is a known cost, not an unknown compatibility issue.

A new unit check constructs a 3,029,999-byte, 30,000-line source document, inserts one character at EOF, confirms the editor reports one local insertion delta (rather than requiring a whole-document replacement), and requests a 40-row snapshot near EOF. The returned snapshot is bounded by the requested range (11 rows remain because the document ends). It completes in 0.17 seconds in the local debug test run, including test setup and assertions; treat that as a smoke measurement, not a latency benchmark. This exercises the kernel only, not DOM render latency or heap usage.

**M0 recommendation:** use `editor-core` as the M1 integration engine, but keep the production CodeMirror path and require the input-host gate before enabling the Rust view for users. The prototype now has a small offscreen textarea sink: Dioxus composition events update a preedit overlay and commit text into Rust; the sink's selection is never read. Synthetic browser composition events and browser clipboard paste pass. Next verify real Japanese/Chinese input on browser and desktop WebView, including dead-key and cancellation behavior. Browser copy/cut and a Rust native clipboard path are implemented; verify native clipboard behavior on KDE/Wayland. Extend scrolling with pixel-precise and wrapped-row mapping; verify Unicode-width hit testing and word/touch drag; check Android keyboard feasibility before treating the editor as feature-ready. The rendering/caret/selection model must remain Rust-owned.

## Reproduction commands

From the repository root:

```sh
cargo test -p moonkale-code-view
cargo check -p moonkale-code-view --example editor_core_spike --features spike-desktop
cargo check -p web --features web --target wasm32-unknown-unknown
```

To launch the standalone browser host and run its Playwright interaction check (with `dx` and Node's `playwright` package installed):

```sh
cd packages/web/tests/fixtures/editor-core-spike
dx serve --web --port 8091 --open false
```

In another terminal, from the repository root, run `PORT=8091 node packages/web/tests/e2e/editor-core-spike.mjs` in an environment where `playwright` resolves. The browser test checks the virtual viewport, cursor-driven reveal, Rust-owned keyboard editing and selection, undo, pointer drag, synthetic composition events, and text delivered through the input sink. It does not emulate an operating-system IME. The unit suite also checks the 3 MB bounded snapshot and local edit delta.

Run `PORT=8091 node packages/web/tests/e2e/editor-core-spike-clipboard.mjs` to check browser clipboard copy, cut and paste with Chromium. Native shortcuts use the Rust `arboard` system clipboard path because the Dioxus Desktop clipboard event adapter does not expose native clipboard data. The Wayland clipboard set/get probe passed; Ctrl/Cmd+C/X/V still need manual acceptance in the desktop spike.

## Source references

- [`editor-core` 0.5.0 README and API](https://docs.rs/crate/editor-core/0.5.0/source/README.md)
- [`dioxus-code-editor` 0.1.2 API](https://docs.rs/dioxus-code-editor/0.1.2/dioxus_code_editor/)
- [`ratatui-code-editor` 0.0.6](https://docs.rs/ratatui-code-editor/0.0.6/ratatui_code_editor/)
- [`freya-code-editor` source](https://github.com/marc2332/freya/blob/main/crates/freya-code-editor/src/editor_ui.rs) and [`freya_edit` API](https://docs.rs/freya-edit/0.4.0/freya_edit/)
- [Freya 0.4 README](https://github.com/marc2332/freya/blob/main/README.md) (Freya 0.4 uses its own reactive core rather than Dioxus)
- [`kode-core` 0.2.0 API](https://docs.rs/kode-core/0.2.0/kode_core/)
- [`taino-edit-dioxus` 0.7.0 API](https://docs.rs/taino-edit-dioxus/0.7.0/taino_edit_dioxus/)
