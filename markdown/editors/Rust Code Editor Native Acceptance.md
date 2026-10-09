---
title: "Rust Code Editor Native Acceptance"
description: Reproducible real-server and native fixture checks, with remaining acceptance gates.
tags: [editors, code, rust, testing]
---

Acceptance recorded 2026-10-07. Rust remains opt-in.

| Check | Result | Scope / remaining gate |
| --- | --- | --- |
| Live rust-analyzer 1.98.1 | Pass | Shared stdio/protocol: diagnostics, change, hover, completion, definition, references, rename, code actions; Unicode/CRLF UTF-16 positions. |
| Linux session mount | Pass | Fixture creates production Workspace text. Dioxus defaults use X11 on this Wayland session; direct Wayland-backend interaction has not been accepted. |
| Native XWayland keyboard | Pass | OS typing, keyboard selection, undo/redo and editable tail after resize. |
| Native Unicode clipboard | Pass | Real arboard copy/cut/paste, including emoji/CJK; previous text clipboard restored. |
| Native pointer focus | Pass | OS click focuses editing. |
| Native pointer drag | Pass | Real OS forward/backward selection, replacement and undo; cursor-word visibility no longer shifts the viewport. |
| Default Dioxus renderer | Probe passes | Native DOM focus/edit/selection/undo/resize and an updated pixel capture pass without externally supplied renderer flags. Current PID-scoped OS retry cannot find a mapped owned window; it is not counted as an OS-input pass. |
| Web production regression | Pass | Workspace suite, including Unicode, synthetic composition, clipboard, folding, language indentation, history and 3 MB virtualization. |
| Real IME, accessibility, theme changes | Pending | Earlier spike/manual Pinyin and synthetic browser composition do not accept the current integrated component. |
| Android, macOS, Windows | Not run | No acceptance on these targets in this continuation. |
| Native UI with live LSP | Pending | Native UI uses deterministic fixture server; live server is tested independently through the shared protocol client. |

After the stable context-row fix, the browser 3 MB check measured 886 ms with 20 rows and 2080 cells in a debug build. This is a regression observation, not the M5 comparative performance benchmark.

Run the real server test from the repository root:

```sh
cargo test -p moonkale-lsp-local --test rust_analyzer --locked -- --nocapture
```

It skips when discovery finds no installed server; acceptance requires an actual installed executable. In this run `rust-analyzer --version` reported `1.98.1 (48a229c 2026-09-01)`, and the test ran against it.

Build and launch the production-panel desktop fixture from the repository root:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo build \
  --manifest-path packages/web/tests/fixtures/native-editor/Cargo.toml \
  --no-default-features --features desktop --offline
MOONKALE_NATIVE_REPORT=/tmp/moonkale-native-canonical.txt \
  target/debug/moonkale-native-editor-host
```

The fixture embeds the production CSS for direct cargo launches, avoiding missing unbundled `asset!` styles. Report instrumentation is enabled only by `MOONKALE_NATIVE_REPORT` and writes canonical Workspace text plus measured first-cell geometry. Production components contain no new JS event bridge.

On KDE/XWayland with `xdotool`, Python and `qdbus6`/Klipper, identify the fixture window and pass its ID:

```sh
xdotool search --onlyvisible --name '^Dioxus App$'
python3 packages/web/tests/e2e/native-editor-desktop.py WINDOW_ID \
  /tmp/moonkale-native-canonical.txt
```

The script sends real OS input and restores the previous text clipboard in `finally`. Its default GTK menu offset is 27 pixels; use `--menu-height` when different. Wait for the geometry report before starting. The fixture's initial text must contain its emoji/CJK sample.

Pointer dragging is included in the standard script. Native event observation identified a layout shift: placing the caret inside a word inserted a context row above the source during mouse-down. The panel now always reserves one fixed-height context row and prevents long words from wrapping. Forward/backward native selection passes, and a browser regression asserts unchanged source geometry when the label becomes visible.

### Default renderer investigation

The earlier blank-window attribution was not established by the controlled tests. Both default and software-renderer OS attempts received no injected events in this continuation. A default native DOM probe then passed focus, editing, selection replacement, undo and resize, and its updated `pfn main()` text was visually captured. Concurrent fixture instances also stalled initialization; the isolated runner refuses concurrent fixtures and matches windows by process ID rather than title.

Run the isolated default-configuration probe after building:

```sh
python3 packages/web/tests/e2e/native-editor-renderer.py \
  target/debug/moonkale-native-editor-host
```

This removes externally set renderer/backend flags but retains Dioxus 0.7.10's own Linux defaults. Its local `Config::new()` enables the existing Linux compatibility behavior: X11, plus disabling the DMA-BUF renderer on eligible Wayland sessions. It does not disable compositing or add a production GPU workaround.

Optional physical-input and pixel checks use the owned process's mapped window:

```sh
python3 packages/web/tests/e2e/native-editor-renderer.py \
  target/debug/moonkale-native-editor-host --os-input \
  --screenshot /tmp/moonkale-renderer.png
```

In this continuation the isolated DOM mode passes, but the optional mode stops at mapped-window detection. A DOM pass is not OS/IME acceptance. The runner cleans up only its own fixture process, including on timeout; `native-editor-desktop.py` rejects reports from a different process. Manual default-configuration OS input, IME, accessibility and other pending matrix entries remain necessary before switching the default editor.

Validation for this investigation: desktop fixture build and Clippy with `-D warnings`, isolated default native DOM probe, Python/JS syntax and formatting/diff checks pass. The web fixture check passes with an unrelated existing `Settings::env_overrides` missing-documentation warning.


2026-10-08: The feature-only proportional run now integrates with the Workspace surface and sparse row heights. Browser acceptance covers marked Unicode search, indexed heights, bounded scrolling and edit/undo; existing proportional/layout/decoration tests pass. Native WebKit DOM acceptance also measures scalar marked spans and routes synthetic input to the shared document. These checks do not establish OS-input/IME acceptance or whole-document proportional navigation.


Measured navigation follow-up: browser Up/Down round trips, preferred pixel column, grapheme boundaries, Shift selection/replacement/undo and same-sink synthetic composition pass. The native DOM driver also passes measured Down/Up on wrapped marked source. These remain opt-in and do not close real OS/IME acceptance.


Visual-edge follow-up: native WebKit DOM acceptance passes Home/End and Left/Right wrap-affinity transitions, measured Up/Down and shared synthetic input. Browser acceptance additionally covers Shift+End replacement/undo and mixed measured/ordinary pixel columns. These tests do not establish real OS/IME acceptance, which remains open.


Font follow-up: native WebKit passes font generation changes and pointer hits using new advances, alongside edges/navigation/shared synthetic input. The initial fast-local-font check was intermittent; the shared font-set metadata fallback resolves that path and three consecutive native runs pass. Browser coverage includes delayed/failed/preloaded replacements, old-generation rejection, caret/resize stability and watcher removal. Real OS-input/IME acceptance is still open.


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
