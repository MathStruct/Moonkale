---
title: "Rust Code Editor Live Preview Plan"
tags: [editors, markdown, architecture]
---

This is a view-layer preparation plan for ADR-0017, read from `dev-claude` commit `db1ae337`. The ADR is not yet present in this worktree. It does not implement markdown live preview or change the current acceptance status. Markdown text remains canonical; editor-core, Workspace history, `ext-api::editor`, UTF-16 positions and LSP ownership remain unchanged. Milkdown stays available until a future preview implementation meets its replacement gates.

## Layout boundary

Introduce a view-owned layout module before adding preview providers. It maps source positions to visual rows and pixel coordinates. Do not teach the text engine about headings, widgets or proportional fonts.

Today `native_surface.rs` calculates viewport rows, restoration, scrolling, canvas height and row placement using a single `row_pixels`. Route all of these through a row-height index with `row_top`, `row_height`, `row_at_y`, `total_height` and viewport-range queries. Uniform code rows retain constant-time arithmetic and no per-row allocation. Mixed rows use a chunked prefix-sum index, with measured overrides and binary search for the visible range. Folding, wrapping and widget insertion invalidate affected row mappings; layout results carry document revision and layout generation so late measurements cannot update a newer layout.

Preserve the visible source anchor and its pixel displacement when measurements change, rather than retaining a visual row number alone. Reveal, caret visibility, scroll restoration and pointer selection must use the same index. Block widgets have a source anchor and display identity; they do not acquire editable document offsets.

Centralize horizontal geometry in the same module: source offset to x, x to source offset with affinity, and pixel-width wrapping. The current monospace implementation delegates source-to-cell mapping to the existing engine, including tabs and wide Unicode characters. A future proportional implementation supplies measured run geometry. Keep canonical UTF-8 offsets distinct from visual cells and convert to UTF-16 only at the existing contract boundary.

## Browser geometry boundary

Defer choosing between span measurement and caret hit testing until a proportional-row spike. If browser geometry is necessary, put the narrow adapter in one module and document the choice. It returns geometry or source-hit candidates for an identified mounted row, revision and layout generation. Rust validates those results and owns caret, selection and edits. A DOM offset is never accepted without mapping it through the row's source spans; hidden markup and widgets require explicit mappings. IME and normal input continue through the existing input adapter.

## Decorations

Define a shared view-only decoration representation with stable provider/decoration identities, source ranges or anchors, revision and deterministic ordering:

- **Mark:** style a source range, allowing overlaps such as syntax, search, diagnostics and wiki links.
- **Replace:** hide a source range or display an inline widget with explicit boundary affinity.
- **Line:** style a source row without changing its text.
- **Block widget:** insert a measured display row anchored between source positions.

The compositor clips decorations to the visible window and resolves conflicts consistently. Replacement overlaps need a declared priority and cannot silently hide an active selection or composition. Providers never write the document or own undo. First migrate existing marks without changing their appearance; add replacement and block rendering only when their layout and input behavior are verified. Markdown and doc-comment providers come later, using the existing arborium grammar infrastructure and language syntax trees.

## Cursor, selection and composition policy

Proposed policy: entering a decorated construct reveals its source before placing the caret. Clicking a replacement chooses the nearest source boundary, then reveals that construct. Arrow navigation into a hidden range reveals it, so every source position remains reachable. Noneditable block widgets are skipped during caret navigation and map pointer hits to their declared source anchor. Selection always refers to canonical source text; copying includes source markup.

During composition, freeze preview changes for the composing construct and keep its source visible. Never place the input sink or composition caret inside hidden content. Rebuild decorations after composition commits or cancels. This policy must be verified before replacement providers ship; it is not implemented by this planning document.

## Implementation sequence and gates

1. Extract the uniform geometry boundary and route scrolling, reveal and hit testing through it. Existing code-editor behavior and bounded rendering must stay intact.
2. Add the mixed-height index with a fixture-only taller row and block widget. Verify scrolling in both directions, resize, measured-height changes, folding, reveal and remount anchoring. Keep the large-file uniform fast path.
3. Introduce the decoration compositor and migrate search, diagnostics and wiki marks. Verify overlapping ranges and stale provider results without changing document text or history.
4. Spike proportional layout and the narrow browser geometry adapter. Verify tabs, emoji, combining characters, wrapped runs, selection and clicks against measured layout on web and native WebKit.
5. Verify replacement navigation, selection, copy and real IME composition. Only then implement markdown and doc-comment providers under a separate feature plan.

Native input acceptance and default-cutover gates in [[Rust Code Editor Native Acceptance]] and [[Rust Code Editor Roadmap]] remain open independently. This preparation does not make live preview a prerequisite for retiring CodeMirror, and does not authorize retiring Milkdown.

## Uniform layout extraction — 2026-10-07

Step 1 is implemented in `packages/code-view/src/native_layout.rs`. The surface routes viewport measurement, scroll restoration, scroll event coordinates, canvas and row geometry, caret reveal and character midpoint hit testing through `UniformLayout`. It retains constant-time, allocation-free arithmetic and delegates source-to-visual mapping to editor-core. Mixed row heights, proportional layout and decorations remain subsequent steps.

## Mixed-height index and fixture — 2026-10-07

`RowHeights` now supplies vertical geometry for the production surface. It uses sparse cumulative height differences and binary search for mixed layouts; uniform layouts allocate no row entries. Rendering, scroll coordinates, viewport clipping and keyboard reveal use this index. Height changes preserve the current visual row and its pixel displacement within that mounted surface.

The separate host enables the `layout-fixture` Cargo feature and provides a fixture context. Its controls insert a 110 px block before logical line 1, increase that text row by 44 px, grow the block, and restore uniform layout. Clicking the block places the source caret at its anchor; typing and undo still use Workspace history. Hidden folded anchors contribute no height. No production host enables the fixture feature.

`native-editor-layout.mjs` covers mixed scroll coordinates, growing a preceding block, remount row restoration, reveal, block input and undo, resize/tail editing, uniform reset and folding. Index tests compare boundaries against a linear height reference and cover empty/large uniform documents.

The initial slice used supplied fixture heights; measured heights and retained source anchors are now implemented as described below. Proportional text and the decoration compositor remain unimplemented. Native fixture compilation passes; OS input and real IME acceptance remain separate gates.

## Measured widgets and source anchors — 2026-10-07

The fixture block measures its mounted element through Dioxus 0.7 `MountedData::get_client_rect`, both after mounting and on resize. Measurements update the height index without changing the source or undo. Acceptance checks the request sequence, mounted element identity, document revision, width, fixture identity and height-index snapshot. Detached, superseded and invalid geometry results are rejected. A fixture delay exercises an old measurement arriving after the widget is reset and replaced.

Viewport state now retains fractional pixel displacement and a source position alongside its visual row. View-only rewrap and folding remap that source position; explicit scroll/reveal changes and text revisions establish a new anchor. This avoids accumulating source-position drift across repeated rewraps. Cleanup releases the added root-scoped signals with the rest of the document view state.

Browser coverage passes for DOM height changes, delayed stale results, fractional remount restoration and preserving the visible source line across width changes, alongside the existing mixed-layout suite. Uniform editor regression coverage and desktop fixture compilation remain required. These callbacks are still fixture-only; general decoration widgets and real IME acceptance remain future work. The shared decoration representation and mark migration are now implemented as described below.

## Shared decoration interface — 2026-10-07

`native_decorations.rs` defines revision-bound provider batches with identities `(provider, id)` and four presentation kinds: mark, replace, line and block widget. Offsets use the normalized editor-core scalar coordinate space. Marks contain typed presentation metadata; widgets have opaque identifiers, and a replacement with no widget represents hidden source. Replacement and general block-widget rendering are deliberately inactive until navigation, hit testing and composition are verified.

Search, diagnostics and wiki adapters now emit this representation. The compositor rejects mismatched revisions, filters to the source/line viewport and orders by provider and provider-local identity. Overlapping marks coexist. Search/current-search flags combine; the first diagnostic and wiki mark keep the existing tooltip/action precedence. Zero-width search emits a point indicator; a zero-width diagnostic marks its source position and retains its gutter entry. Diagnostic UTF-16 coordinates are converted to engine offsets before composition. NativeCell consumes the combined marks rather than scanning each provider separately.

Search uses binary search to materialize only candidate matches. Wiki marks and diagnostics are filtered before cloning; composition preserves the original ranges instead of truncating them into artificial point decorations. Source text, selection, input, undo and LSP lifecycle are unchanged by the decoration interface.

Unit coverage verifies overlapping marks, stale revisions, viewport boundaries, diagnostic points/gutters, deterministic precedence and clipping all four kinds. `native-editor-decorations.mjs` checks simultaneous Unicode search/diagnostic and wiki/search classes, tooltip preservation and unchanged canonical text. Existing diagnostic, wiki and workspace suites cover clearing, stale LSP versions, regex/zero-width search, navigation and history.

Broader verification exposed a view-prop subscription bug from the earlier layout work: a reused component's height memo retained the previous model signal. The memo and viewport effects now use Dioxus 0.7 `use_reactive` for changing signal handles; periodic measurements resolve the current model/viewport handles when called. The layout browser suite includes Julia-to-Lean switching followed by folding a proof and checking its visible sibling height.

The next slice is the proportional-text layout spike and a narrowly scoped browser geometry adapter. General replacements, preview widgets, markdown providers and real IME acceptance remain future work.

## Proportional geometry spike — 2026-10-08

`ProportionalGeometryProbe` is an isolated, opt-in fixture component backed by editor-core. It displays a proportional text run wrapped by CSS pixel width, draws caret and selection from measured geometry, and sends pointer hits and keyboard/input commands to Rust. A 4096-scalar input limit bounds measurement cost. The production code surface retains its existing monospace layout; this spike does not implement markdown editing or change Workspace history/contract.

### Browser geometry decision

Use `Range.getClientRects` through one adapter module, `native_browser_geometry.rs` and its included `native_browser_geometry.js`. Rust segments the source into graphemes and sends each segment's engine-scalar and DOM UTF-16 boundaries. The browser measures those ranges and returns rectangles relative to the mounted source run. It does not read DOM selection, infer document edits, own caret state or mutate source. This makes combining sequences and emoji indivisible pointer targets, and measures proportional glyph advances, tabs and CSS wrap directly. Source strings travel through Dioxus's serialized eval channel, not interpolated JavaScript.

Rust supplies offset-to-caret geometry with forward affinity at wrapped boundaries and x/y-to-source hit testing. Pointer selection uses normal engine selection commands; replacements and undo use engine edits. Invalid/nonfinite/out-of-bounds geometry is rejected. Request sequence, source identity and mounted component lifetime reject obsolete results; hit testing requires geometry matching the current source and configured width. Redundant resize measurements can retain matching geometry without dropping clicks.

### Verification and limits

`native-editor-proportional.mjs` passes proportional W/i widths, narrow/wide pixel wrapping, emoji/combining/CJK/tab clicks, repeated input, pointer selection/replacement, undo and delayed stale geometry. Geometry unit coverage checks grapheme boundaries, wrapped caret affinity and invalid values. The native fixture's `MOONKALE_PROPORTIONAL_PROBE` flow also passes Range geometry, wrapping, hit testing and synthetic DOM editing through actual WebKit. Run it with `native-editor-proportional.py`; it starts and cleans up only its owned fixture process. This is not OS-input or real IME acceptance.

The spike renders one bounded plain text run. Integration into the production virtualized surface, styled/hidden source span mappings, visual up/down navigation, bidi, general multiline/empty-run behavior, browser font-load invalidation and full input/composition/clipboard support remain gates. Its undo is the isolated engine's history, not a second production Workspace history. The next implementation step is connecting measured proportional runs to the existing source/visual row index and decoration mappings, with cursor and composition policy verified before replacement providers ship.


### Virtualized proportional source integration (2026-10-08)

The `layout-fixture` opt-in now renders a selected logical source line through `ProportionalRun` in the existing Rust Workspace surface. Shared search, diagnostic, wiki and syntax classes remain on scalar source spans; the geometry adapter walks their text nodes and measures graphemes across span boundaries. Selection, caret and zero-width search overlays stay outside measured source. Pointer commands and typing use the existing engine and Workspace edit/undo path.

Measured heights update the retained sparse row index, with document revision, source row and viewport width checks. Other rows keep the uniform renderer. The fixture limits the logical line to 4096 scalars, requires wrapping and falls back for fold placeholders. Existing engine run boundaries remain; CSS wraps within each run. This is not whole-document pixel-based wrapping. Visual up/down navigation, bidi, font-load invalidation and OS/IME acceptance remain gates before production use or markdown replacement providers.

Validation: 82 unit tests, default build and feature Clippy; browser integrated Workspace edit/undo, marked Unicode search, measured heights and bounded scrolling; isolated proportional, mixed-height and decoration regressions pass. The integrated browser entry point is `native-editor-proportional-integrated.mjs`. The native WebKit driver also verifies marked-span measurement and shared Workspace synthetic input before its isolated Unicode probe; desktop build and fixture Clippy pass.


### Measured vertical navigation (2026-10-08)

The opt-in Workspace surface now handles Up/Down and Shift+Up/Down using measured grapheme caret boundaries. It retains a preferred pixel column across uninterrupted vertical moves, including adjacent engine source runs. Pointer placement and other commands reset that preference; revision, model and viewport-width checks prevent stale geometry from supplying targets. Only neighboring measured runs participate. Outside the measured line, or while geometry is unavailable, existing engine navigation takes over. Completion-menu arrows and composition keep their existing handling.

`native-editor-proportional-navigation.mjs` verifies actual Unicode wrapped geometry, repeated Down/Up, preferred-column retention, Shift selection/replacement and Workspace undo. Synthetic composition coverage verifies stable source caret during composition and one commit for compositionend plus the following input on the same sink. The native WebKit probe now also checks measured Down/Up across marked spans before shared input. This remains DOM acceptance, not real OS/IME acceptance.

Next gates: source-boundary affinity and visual Home/End across wraps, transitions between measured and uniform text with pixel-column retention, invalidation after fonts load, and real OS/IME acceptance. Whole-document pixel wrapping, bidi and replacement/widget presentation remain deferred.


### Visual edges and mixed-layout movement (2026-10-08)

The bounded proportional fixture now implements Home/End and Shift+Home/End on the displayed CSS row. Wrap affinity is view state: End can draw the caret after the preceding grapheme even when that source offset also starts the next displayed row. Left/Right crosses that affinity without skipping source. Grapheme positions remain engine scalar offsets; selections and replacements still use Workspace commands/history. Preferred pixel columns now survive transitions between measured and ordinary rows, including short-row clamping. Uniform targets use engine cell widths for tabs/wide characters and expose only grapheme boundaries. Completion, composition and primary-modifier document Home/End retain existing behavior.

Edge acceptance exposed an adapter issue at styled-span endpoints: ending a Range at the next text node's offset zero could include its rectangle. End boundaries now resolve to the preceding text node's end, keeping measurements tied to their source grapheme. Zero-width trailing source ties choose the complete displayed row boundary.

`native-editor-proportional-edges.mjs` passes Home/End caret placement, bidirectional wrap arrows, Shift replacement/undo and measured/ordinary pixel-column transitions. Navigation, integrated editing, isolated geometry, mixed-height and decoration browser regressions pass; 84 unit tests and default/feature Clippy pass. Native WebKit DOM acceptance also passes Home/End, wrap-affinity arrows, measured Up/Down and shared input. Real OS/IME acceptance remains open.

Next: invalidate geometry when fonts load, verify resize/font transitions and affinity lifetime, then real OS/IME acceptance. Whole-document pixel wrapping, bidi and replacement/widget rendering remain pending. The feature stays opt-in, bounded to one logical line; markdown providers and default-backend changes are not enabled.


### Font generations and resize stability (2026-10-08)

Measured run keys now include a font generation. A document-level watcher shared by mounted views observes FontFaceSet loading/done/error and initial readiness. A 500 ms metadata fallback tracks face identity, descriptors and load status, covering fast local faces and already loaded add/delete/replacement operations that need not emit loading events. Source-node removal releases its watcher; the last watcher disconnects the observer, removes font listeners and stops the fallback timer. Font generation changes reject old measurement results and navigation caches. During font loading the bounded measured line rejects pointer/arrow targets while retaining source and existing indexed heights until fresh geometry arrives.

Resize clears outdated run geometry and navigation entries. Duplicate size notifications retain matching geometry, avoiding readiness flicker. Fresh measurements rebind backward wrap affinity to the source offset when it remains a wrap boundary, or clear it when the new layout makes that offset an ordinary boundary. Source caret, canonical text and Workspace history remain unchanged by font/width changes. The isolated proportional probe uses the same font watcher.

`native-editor-proportional-fonts.mjs` uses existing local KaTeX font assets to verify real delayed loads, failed-load fallback and preloaded face replacement; it checks old-result rejection, stale pointer/navigation protection, fresh glyph advances, caret coordinates after resize, remount and watcher removal. Browser proportional/edge/navigation/integrated/layout/decoration regressions pass; 84 unit tests and default/feature/desktop fixture Clippy pass. Native WebKit font invalidation, fresh-font pointer hits, edges, navigation and shared synthetic input pass three consecutive runs after the local-face fallback was added.

Next gates are real OS-input/IME acceptance and broader proportional layout/navigation coverage before replacement/widget rendering and Markdown providers. Whole-document pixel wrapping and bidi remain pending. This remains a one-line, 4096-scalar, opt-in fixture; default code/Markdown backends are unchanged.


### Native OS-input diagnosis (2026-10-08)

The renderer runner now provides explicit `--x11` acceptance, choosing GTK X11 before launch while preserving a separate default-renderer path. Default launch has no PID-owned X window discoverable by xdotool in this session; explicit X11 produces one and captures pixels successfully. No production renderer setting changed.

The fixture reports final cell geometry after the DOM resize; the OS script focuses the owned window and clicks inside the measured first cell. Fixture-only document listeners observe trusted pointerdown, keydown and input events and send a listener-ready receipt. On failure the runner prints their counters. The current explicit-X11 OS attempt still fails at initial typing: the sink is focused, listeners are ready, and all three trusted-event counters remain zero. Canonical source is unchanged. This establishes lack of observed event delivery, without attributing it to editor or compositor behavior. Fcitx5 is running, but real IME acceptance cannot be established until native input reaches WebKit.

Native fixture build and Clippy, Python/JavaScript syntax and fixture formatting pass. X11 DOM checks and owned-window screenshot capture pass. Real OS input and IME remain open; proportional DOM acceptance is separate. Run `python3 packages/web/tests/e2e/native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --os-input` to reproduce. Owned fixture processes are cleaned up and the clipboard is restored on failure.


### OS input reached the fixture after user tool-button action (2026-10-08)

After the user reported clicking the xdo tool button, an explicit-X11 renderer run passed the complete OS suite: pointer focus, both drag directions, typing, keyboard replacement, Unicode clipboard, undo/redo and resized input. The connection between that action and recovery is not established. A confirmation run received trusted events (48 keydown, 2 pointerdown) and progressed through the clipboard/history checks, but failed restoring source with undo after a drag replacement: canonical source remained `Qveprobepfn main() {\n    // 😀中 Unicode\n}\n`. Thus the earlier zero-event observation no longer describes every run, but OS acceptance is intermittent and cannot be marked consistently passing. Real IME and proportional-wrap OS acceptance remain pending. Both owned processes exited through runner cleanup.


### Measured-wrap OS acceptance and IME follow-up (2026-10-08)

Added `native-editor-proportional-os.py` and renderer `--proportional`/`--ime` modes. The fixture initializes one measured logical line through DOM controls; all acceptance edits/navigation use real X input. Fixture observation streams report canonical source, caret offset/rectangle, readiness, composition state and trusted event traces. Traces retain 80 events with bounded source excerpts, key/modifier/data/target information; failure output includes native runtime logs. No production editor behavior or graph-editor files changed.

Explicit-X11 proportional OS acceptance passed multiple runs: pointer focus, visual Home/End, same-source-offset wrap arrows, measured Down/Up, Shift selection/replacement, multi-character typing, select-all replacement and Workspace undo. The uniform real-input suite also passed its retry, including forward/backward drag and undo. Intermittent failures remain: zero-event delivery in some runs, one trace lacking the final typed `p`, and one startup geometry-readiness timeout. The earlier drag-undo failure is not established as a history bug and was not patched speculatively.

Fcitx5 keyboard-de and Pinyin are configured; the installed GTK module registers both fcitx/fcitx5. The IME mode explicitly connects GTK to Fcitx, temporarily selects Pinyin and restores the previous active method in child and parent cleanup. It requires unchanged source during preedit, exactly one Chinese commit and Workspace undo. Real attempts reached the IME stage after passing proportional OS checks but failed at preedit: the trace showed an Unidentified key and no expected compositionstart/update sequence. Real IME acceptance remains open; its root cause is not established. Another attempt without explicit GTK binding observed raw nihao input/compositionend, which is not accepted as Pinyin composition.

This turn's launch environment initially lacked DISPLAY/WAYLAND_DISPLAY and reported XDG_SESSION_TYPE=tty; local session authorization had changed. Tests used the current session's DISPLAY=:1, WAYLAND_DISPLAY=wayland-0, XDG_SESSION_TYPE=wayland and renewed XAUTHORITY. Wrong session metadata first caused initialization/GBM failure; production renderer defaults remain unchanged. Fixture build/Clippy, formatting and Python/JavaScript syntax pass. Owned test processes are cleaned up, and the graph editor directory remains untouched.


Final verification: default native DOM acceptance passed. The separate proportional DOM regression failed this time while waiting for all mounted runs to become ready: source starts 0/85/172 were ready, 258/344 remained unready with fonts loaded and epoch 1. This is an unresolved geometry-readiness failure, not a passing regression. Final Fcitx-bound OS runs passed measured navigation/editing but failed Pinyin preedit, including when injecting individual symbolic keys; no native runtime error was logged. Active input method was verified restored to keyboard-de and no owned fixture remained. Next: reproduce the unready trailing runs and isolate real IME behavior with a visible textarea control before changing production input policy.


### Geometry and real IME isolated and fixed (2026-10-08)

The `--isolate` native runner creates a plain visible textarea beside the editor, observes trusted events and compares ASCII delivery, Pinyin source preservation, candidate commits and editor undo. Connected measured runs expose receipt/status diagnostics and direct DOM rectangle counts. This reproduced two concrete causes:

* Geometry: a valid browser measurement was followed by `receive: EvalError::Finished - eval has already ran`. The installed Dioxus 0.7.10 native evaluator closes its JS channel on script completion and drops the Rust query owner on channel GC, which can invalidate a queued measurement before Rust receives it. The geometry adapter now waits for a Rust acknowledgement after sending its reply; Rust acknowledges immediately after receive, including a null/malformed-result path. Existing source/revision/width/font/stale-request checks remain intact.
* IME: both the plain textarea and editor receive a trusted Unidentified key followed by input(`你好`) and compositionend(`你好`), with no compositionstart/update in this WebKit/Fcitx path. The control commits once; the editor previously produced `你好你好`. NativeSurface now rejects compositionend from a sink generation already consumed by input. The browser navigation regression covers both start/update/end/input and input/end without start/update; each commits once and undoes correctly.

After both fixes, isolation showed all measured runs accepted/ready, both controls committed `你好` once, and editor undo restored source. The full measured-wrap OS plus Pinyin suite passed source preservation, single commit and Workspace undo. The native WebKit geometry/font/edge/navigation DOM regression passed. Browser navigation/composition-order, edges/mixed-row navigation and delayed/failed/preloaded-font/resize suites passed. All 84 code-view unit tests and native-desktop/layout-fixture all-target Clippy passed; fixture build, syntax and formatting passed.

Earlier zero-trusted-event and missing-OS-key observations remain separate environmental/automation limits; these fixes do not establish a universal platform matrix or expose preedit events absent from the reference control. The Rust backend and proportional fixture remain opt-in. Graph-editor files were untouched. Next broader proportional coverage, then replacement/widget presentation gates; Markdown providers remain deferred.


### Broader proportional Unicode coverage (2026-10-08)

Added the Unicode navigation fixture and `native-editor-proportional-unicode.mjs`. It covers combining accents, skin-tone/ZWJ emoji, flags, Hangul, Indic text, CJK and tabs; whole-grapheme run boundaries; pointer hits and Left/Right/Shift traversal; replacement/Workspace undo; CSS visual Home/End and Down; preferred-column movement through one-character, empty and tabbed uniform rows; and narrow/wide reflow at 340/670/950px. The source remains unchanged after navigation and edit/undo checks.

The first run reproduced an engine source-wrap boundary between `👩` and the remainder of `👩🏽‍💻`. Fixed the vendored editor-core layout routines to iterate extended graphemes in both character and word wrapping, preserving existing scalar offsets, per-character cell widths, global tab stops and continuation indentation. Oversized graphemes stay intact on a nonempty segment rather than creating a leading empty row. No view-side source ownership adjustment is needed. A headless regression checks complex-cluster boundaries, byte/scalar mapping and monotonic nonempty wrap points for widths 1–24 in both modes with indentation.

Validation: 133 editor-core unit tests and eight engine doctests; 84 code-view unit tests; new Unicode browser suite and existing navigation/composition, edges/mixed rows and font/resize suites; native WebKit proportional geometry/font/navigation regression; default and native-desktop/layout-fixture all-target Clippy; fixture desktop build, formatting and JavaScript syntax all pass. Proportional rendering remains a bounded opt-in logical-line fixture. No graph-editor files, Markdown providers or backend defaults changed. Next is replacement/widget presentation and its source-to-visual mapping gates; whole-document pixel wrapping, bidi and broader platform acceptance remain pending.

Final confirmation: real XWayland proportional OS input and Fcitx5 Pinyin single commit/Workspace undo pass after the grapheme-wrap change.


### Bounded replacement and widget presentation (2026-10-08)

The existing Replace and BlockWidget decoration kinds now render through an opt-in synthetic provider. `LayoutFixture.presentation` enables it; `#layout-presentation` supplies hidden `[[hidden]]`, an inline `[[chip]]` label and a 72px measured noneditable block anchored at source offset zero. `native_presentation.rs` holds provider output and grapheme validation. Composition remains revision-bound and provider/id ordered; overlap losers and replacements crossing a measured source-run boundary fall back to canonical source rather than clipping or duplicating widgets.

ProportionalRun renders explicit scalar source-start/end mappings. The geometry adapter verifies the full mapping against Rust-supplied canonical source and widget labels, returns one source-boundary box per replacement, and keeps normal grapheme mapping for visible text. Inline labels acquire no editable offsets or history. Hidden ranges have collapsed boundary geometry. The existing shared input sink, engine selection, Workspace deltas, clipboard and history remain authoritative. The block uses the retained measured-height index and maps pointer hits to its declared source anchor.

For this bounded slice, entering the previewed logical line reveals the entire line; any selection touching it and composition also keep its source visible. Replacement clicks choose the closest start/end boundary before reveal. A presentation bit in RunKey gates navigation, height and caret callbacks, and replacement vectors participate in stale geometry checks. This is deliberately more conservative than future per-construct reveal. Production code/Markdown backends and providers are unchanged.

`native-editor-presentation.mjs` passes hidden/inline/block mapping, active-line reveal, measured block height, pointer/edit/undo, full-source selection/replacement, composition and delayed revision/presentation invalidation. The real native `--x11 --os-input --presentation --ime` suite passes both widget boundaries, typing/undo, copying canonical hidden markup and Pinyin source preservation/single commit/undo. Clipboard and input method are restored. Native fixture report measurements now also use a Rust receipt acknowledgement: the initial attempt reproduced the previously isolated evaluator-finish race in that fixture-only helper. A compact wrapped source keeps the first widget and outside caret simultaneously visible in the native viewport.

Validation: 86 code-view unit tests including invalid grapheme replacements, provider priority, stale revisions, whole-run acceptance and block anchoring; default and native-desktop/layout-fixture all-target Clippy; desktop fixture build/Clippy and syntax/formatting checks; browser presentation, Unicode/reflow, navigation/composition and visual edges all pass. Font/layout/native proportional regression results follow below. Cross-run/multiline replacements, arbitrary widget content/interaction, bidi, whole-document pixel wrapping and per-construct reveal remain future work. Next: extend the explicit source mapping beyond the bounded fixture before introducing Markdown providers. No graph-editor implementation was edited.

Final regression confirmation: browser font invalidation/resize and mixed-height scrolling/anchoring/remount/reveal/folding/stale measurements pass; native WebKit marked geometry/font/navigation passes. All owned native fixtures exited and the web test server was stopped after validation.


### 2026-10-09 — Cross-run replacement ownership

The bounded presentation line now moves view run boundaries inside a replacement to its canonical source end. The owner receives the whole hidden range or inline widget; continuation runs receive only remaining source cells. Mapping uses the complete bounded logical line, including when the virtual window starts inside a continuation. Engine layout, text, selection, history and scroll row anchors stay authoritative. Syntax tags and decoration windows are built after remapping. Fully consumed rows retain their engine spacing but do not create measured runs, avoiding duplicate geometry/cache ownership. Active-line/selection/composition reveal still restores the original source runs.

The presentation fixture now places tokens farther into its line. Browser tests at 340, 480, 500, 520, 550, 670 and 950px prove that both tokens cross engine boundaries, render exactly one inline label, preserve all other source in order, and map both widget sides to full canonical endpoints. Pointer/edit/undo, selection/replacement, composition and stale-result checks pass. A unit test covers a replacement spanning three runs and a virtual window beginning inside it.

Validation: 87 code-view unit tests; default and native-desktop/layout-fixture all-target Clippy with warnings denied; desktop fixture build; browser presentation, navigation/composition and mixed-height layout regressions; native X11 real pointer boundaries, typing/undo, canonical clipboard copy and Pinyin single commit/undo all pass. The native probe source detector was updated for the longer fixture prefix.

Scope remains an opt-in first logical line of at most 4096 scalars. Multiline replacement ranges, compacting fully consumed engine rows, arbitrary interactive widget content, per-construct reveal and Markdown providers remain future work. No graph-editor implementation was edited.


### 2026-10-09 — Compact fully consumed presentation runs

Fully consumed source runs now have zero visual height and create no DOM row, gutter or pointer target. The engine retains its original row IDs and source layout. `fixture_runs` builds one bounded source mapping shared by rendering and height calculation, preventing ownership/height disagreements. The sparse height index accepts finite zero-height overrides, skips collapsed rows for pointer/scroll lookup and finds adjacent visible rows for navigation. True empty source lines retain their normal height. Measured-height cache entries include presentation mode so preview geometry cannot size revealed source.

Added long synthetic `[[hidden:...]]` and `[[chip:...]]` fixture tokens and `#layout-consumed-runs`. `native-editor-consumed-runs.mjs` verifies several complete rows disappear, no pixel gap remains, one widget owns the full canonical range, pointer endpoints and keyboard entry reveal all source rows, edits undo correctly, and delayed measurements cannot restore stale compacted heights. Native `--presentation --consumed-runs` adds source-order/zero-gap assertions; combine with `--x11 --os-input --ime` for clipboard, pointer and Pinyin acceptance.

Validation: 89 code-view unit tests; default and native-desktop/layout-fixture all-target Clippy with warnings denied; desktop build and formatting/syntax checks; browser consumed-row, existing presentation, navigation/composition and mixed-height scrolling/remount/folding suites pass. Native long-range real pointer, canonical copy, typing/undo and Pinyin single commit/undo passed on retry. Its first attempt recorded zero trusted OS events, consistent with the previously observed automation intermittency; it was not attributed to editor input handling. A web run overlapped a fixture rebuild and was rerun against the mounted app.

Remaining scope: multiline replacements, per-construct source reveal, general widget content/interaction and Markdown providers. The preview remains an opt-in logical line capped at 4096 scalars. No graph-editor implementation was edited.


### 2026-10-09 — Per-construct source reveal

Caret entry at either canonical boundary or inside a replacement now reveals only that construct. Nonempty selections reveal overlapping constructs; unrelated source and constructs remain in preview. Composition uses the caret/selection reveal state, so an IME insertion inside one revealed widget leaves the other range hidden. Source outside both constructs no longer reveals an entire logical line.

The former presentation boolean in RunKey and retained height entries is now a bounded fixture bit mask. Rendering, cross-run ownership and collapsed-row height calculation use the same active replacement subset. Navigation and asynchronous height/caret callbacks validate the mask against the live cursor/selection state; existing replacement-vector and revision checks remain in place. Only rows consumed by still-previewed constructs remain collapsed.

`native-editor-local-reveal.mjs` verifies caret and local selection transitions, both-construct selection, resizing at 340/550/950px, composition inside a widget with unrelated hidden source, single commit/undo, leaving the construct and delayed stale-mask invalidation. Existing presentation and consumed-row suites now assert partial reveal. Native fixture observation includes visible source; OS acceptance verifies the other range stays hidden during pointer entry and real Pinyin input inside the revealed widget, plus canonical clipboard copy and undo. The long-range native fixture uses a 550px panel so its full-source-order assertion includes the prefix rather than treating a legitimately virtualized prefix as missing ownership; diagnostic errors include visible rows and cursor state.

Validation: 90 code-view unit tests; default and native-desktop/layout-fixture all-target Clippy with warnings denied; desktop fixture build and formatting/syntax checks; browser local reveal, consumed rows, presentation, navigation/composition and mixed-height scrolling/remount/folding suites; native X11 real pointer, canonical copy, typing/undo and Pinyin single commit/undo all pass. All owned test processes were cleaned up. No graph-editor implementation was edited.

The first-line/4096-scalar synthetic provider remains opt-in. Multiline replacements, arbitrary interactive widget content, broader platform coverage and Markdown providers remain future work.


### 2026-10-09 — Bounded multiline replacement ownership

Synthetic hidden ranges and inline widgets can now span logical lines. The provider scans complete source lines in a prefix capped at 4096 scalars, excluding incomplete tokens/lines beyond that budget. The complete mapping covers all affected logical lines and carries canonical newline scalars in an owner extended across a replacement. Ordinary runs still omit their line separators. Each construct has one owner; consumed continuation rows, including genuinely empty source lines inside a replacement, collapse. Per-construct reveal restores original logical runs and empty lines. Source outside replacements retains its engine row IDs and offsets.

The renderer remaps by global visual-row index, and proportional geometry/height retention covers all affected logical lines. Height validation accepts source anchors on those lines and still checks revision, width and reveal mask. Mapping returns to engine ownership when folded regions prevent complete source coverage; a fold/unfold regression checks this fallback. The bounded source budget is shared by provider composition, reveal masks and geometry mapping.

Added `#layout-multiline` and `native-editor-multiline.mjs`. Tests cover newline ownership and unique widgets, empty-line collapse/reveal, local reveal, both widget endpoints at 340/550/950px, typing/undo, crossing selections, navigation, composition single commit/undo, scrolling and delayed stale-mask invalidation. Native runner `--presentation --multiline` selects the same source; `--x11 --os-input --ime` passes real pointer boundaries, canonical clipboard copy including all line separators, typing/undo, and Pinyin input inside the revealed multiline widget while the other range remains hidden.

Validation: 93 code-view unit tests; default and native-desktop/layout-fixture all-target Clippy with warnings denied; desktop fixture build; formatting and JS/Python syntax; browser multiline, earlier local reveal, consumed rows, presentation, navigation/composition and mixed-height scrolling/remount/folding suites; native X11 pointer, copy, undo and real Pinyin all pass. Owned test processes were cleaned up. No graph-editor implementation was edited.

This remains an opt-in synthetic provider over a bounded source prefix, rather than whole-document proportional layout or Markdown preview. General widget content/interaction, real Markdown/doc-comment providers and broader platform acceptance remain future work.

### 2026-10-09 — Interactive block content and focus boundary

The next widget slice is complete behind `layout-fixture`. `PreviewBlock` accepts provider-owned Dioxus children, title/control labels and an explicit source action. It owns expansion and an isolated keyboard/pointer/clipboard boundary. Provider content can contain ordinary native controls; those controls do not route typing, navigation, shortcuts, selection or clipboard events through editor-core. Escape returns to source unless the key belongs to composition. The explicit source action validates the mounted source revision, reconciles pending Workspace changes, places the caret at the block anchor and focuses the shared input sink. Noninteractive blocks keep their existing pointer-to-source behavior.

`LayoutFixture.interactive_widget` and `#layout-widget` enable expandable content with a view-local note field. The block has natural content height with a minimum rather than a fixed clipped height; the existing guarded DOM measurements update the shared sparse row-height index. Accessibility targets include the mounted surface identity, so duplicate panes have independent state and unique IDs. Content remains mounted while collapsed. Its state is view-only and resets when its source revision changes or its component is unmounted; it is not persisted document data.

Native acceptance found that a controlled note field could overwrite newer OS keystrokes with delayed Rust values. The input now owns its native value while Rust observes changes for presentation. The native probe waits for the canonical source rows before acting, rather than racing the initial Workspace-to-model reconciliation.

Verified: browser widget focus, local typing, navigation/shortcuts, clipboard event isolation, composition Escape, duplicate-pane state, canonical caret/undo, delayed measurements, resize and partial scrolling; existing mixed-height layout, multiline presentation and retained-tab integration suites; native WebKit DOM checks plus real XWayland widget typing/navigation, Escape, source typing/undo and Edit source; 99 code-view tests, default/native-desktop/layout-fixture Clippy, desktop fixture build and repository checks. Real OS shortcut checks omit globally intercepted Ctrl-period; browser coverage verifies shortcut isolation. Real widget-input IME, arbitrary inline controls, persistent provider state and real Markdown/doc-comment providers remain later gates. Next: a separate, bounded Markdown provider plan using the verified presentation and widget boundaries.
