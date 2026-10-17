---
title: "Rust Code Editor Accessibility and Cutover"
description: "WP-03 M5: automated accessibility, lossless backend selection, build evidence and staged CodeMirror retirement."
tags: [editors, accessibility, testing]
---

Status: acceptance work on 2026-10-15. **The release default is still CodeMirror.**
This page records the gates that do not need Daniel's desktop or phone. See
[[Rust Code Editor Roadmap]], [[Rust Code Editor Native Acceptance]],
[[Code Editor Requirements]] and [[P-198 Rust editor accessibility and backend activation]].

## Accessibility contract

The Rust engine owns the text, caret, selection and history. The input textarea
is a transport, not a second text document. Its accessible name includes the file
path. It is the single editable textbox; the virtualized source is a named reading
region. One stable polite, atomic status exposes the canonical caret and selection
endpoints plus a bounded current-line excerpt (at most 160 source scalars and a
leading ellipsis). Long lines never create a full accessible textarea mirror.
The hidden input remains empty; this does **not** implement the full native
editable-text document API. Reading arbitrary source uses engine arrow navigation
and its canonical context, or the mounted source reading region.

Primary-M toggles Tab between indentation and focus movement, announced in the
status and explained by the input's help. The default remains indentation.
Shift-Tab can return to the input after leaving it. The transport has a fixed
viewport position so programmatic focus does not scroll the page mid-gesture. Search, rename, completion,
references/actions and hover have keyboard paths; Escape returns to source.
Primary-Enter follows a wiki-link, or a Markdown link/standalone image with preview enabled, at the caret through the same guarded resolver
as pointer navigation. Preview controls retain their own native button/input
behavior. Focus uses theme tokens, and `prefers-reduced-motion` disables editor
animations/transitions and smooth scrolling.

`native-editor-accessibility.mjs` checks the actual Chromium accessibility tree,
not only DOM labels, and a mouse-free editing/tool/focus journey. It checks source
contrast under the production built-in palette with reduced motion: observed
minima **5.54:1 light, 4.58:1 dark**. These are sampled source/gutter colors in the
fixture, not a guarantee for arbitrary user themes or every rendered widget.

`native-editor-accessibility.py` creates a private session/accessibility bus and
owned headless display, then inspects GTK WebKit's actual AT-SPI tree. It verifies
the named input/help and a status-bar text interface containing caret/selection
context with `live=polite`. WebKit exposes dynamic context in the status text;
its input description may contain only the static help. The runner uses the
installed GI/Atspi typelib and `dbus-daemon` and uses short temporary socket paths
(never a temporary build target). It stops its display and session bus and retains
logs under this worktree's target. It never inspects the user's bus or screen.

These checks establish accessibility semantics and keyboard behavior. They are
not a speech-engine transcript, a real OS-input/IME test, or acceptance on Android,
macOS or Windows. Earlier Linux physical-input/IME evidence remains in
[[Rust Code Editor Native Acceptance]].

## One setting, no destructive migration

The existing `editor.implementation` is the only user backend setting:

| Stored value | Resolved behavior |
| --- | --- |
| absent or unknown | `settings::DEFAULT_CODE_EDITOR`, currently `codemirror` |
| `codemirror` | explicit CodeMirror preference |
| `native` | Rust; implicitly activates its shipped extension unless explicitly disabled |
| explicit Rust extension disable | fallback to CodeMirror, including when the global backend preference is Rust |

Resolution does not rewrite the stored file or extension lists. Explicit
per-document toolbar choices win for the current session; they are not new
persisted state. User/workspace/environment precedence is unchanged. The core
Code extension also honors Rust selection when the optional adapter is absent in
a minimal distribution. This keeps the future release-default flip in one public
constant, without requiring a second extension-default edit or another UI switch.
A test deliberately asserts that the release constant is still `codemirror`.

Canonical open documents keep their `NodeId`, source, text and dirty state while
panels change. The Rust view restores the active canonical UTF-16 caret/range on
mount, including reversed selections and CRLF/non-BMP text. Its Workspace history
continues for subsequent Rust edits; CodeMirror's private pre-switch undo stack is
not converted into Rust transactions. Existing Rust history can be invalidated by
external CodeMirror edits according to the existing revision contract. No save,
reload or source write is a side effect of a backend change.

`editor-migration.mjs` checks two live unsaved buffers, exact Unicode/CRLF saves,
active reversed selection, one-setting activation without an extension enable
list, per-document fallback across global changes, and saved-document/layout
restoration after reopening the folder. A browser reload restores saved tabs;
it does not promise crash recovery for unsaved buffers (WP-56 owns that).

## Build and size evidence

`editor-build-cost.py` builds the production-panel desktop fixture into a **new
worktree-local target**, then measures no-change and application-only rebuilds in
the same target/profile/features. Dependencies are already downloaded; the
compiler/toolchain and filesystem caches are not asserted cold. It records wall
time and compiler/linker maximum RSS, not application resident memory. It also
records raw/gzip sizes of the native fixture, current release web server/wasm and
the CodeMirror bundle. The optional `--desktop` phase also builds the actual
`moonkale` desktop binary in the existing, partially cached target, recording its own time,
compiler/linker RSS and raw/gzip size. Reserve bench.lock and both heavy slots; use heavy.sh.

The web bundle directory retains old fingerprinted assets between builds, so its
accumulated disk usage is not the shipped client size. Use the current wasm
artifact and its reachable asset set, not `du` over old fingerprints. Both
backends still ship. The matched CodeMirror-versus-Rust setting comparison uses
**the same artifacts and build**: backend selection is runtime configuration,
not a Cargo feature or rebuild, so switching alone changes build/size cost by
zero. These measurements cannot predict savings from a future native-only build;
that comparison belongs after the zero-consumer removal audit.
The first fresh-target attempt failed after 257.54 s because upstream
`dioxus-code` writes its generated CSS into the read-only Cargo registry. That
failed attempt is retained in `target/next4-build-cost-20261015.json`; it is not a
clean-build pass. The retry uses `--prebuilt-theme-assets` (upstream's `DOCS_RS=1`
skip-generation branch) with already-generated registry CSS. The variable applies
to the fixture and a separate `cargo build -p dioxus-code` theme seed in the
product target. The actual desktop phase removes it because native database
build scripts also interpret it. Cargo reuses this upstream script's output
(it does not watch DOCS_RS); both normal product compilation and linking still
run. The first normal desktop attempt also failed at the registry write after
511.99 s, retained in `target/next4-build-cost-assets-final.json`; that time is
not a successful desktop clean-build result. No registry files/permissions are changed.
The surviving NEXT-4 report ends at that failed desktop phase: its fixture
fresh-target/no-change/application-only phases completed in 213.365/0.217/4.683 s,
with compiler/linker maxima 1,533,996/128,116/673,292 KiB. Its artifacts table is
empty. Those fixture timings were not the final isolated repeat and remain
diagnostic evidence. The proposed isolated repeat, final native report and actual desktop
release binary were never produced; they are not acceptance results. The initial
AT-SPI pass remains `target/next4-native-accessibility-5.json`. The post-link gate
log also ends during compilation; the earlier complete gate log and Claude's
subsequent green merged-base gates/CI are separate evidence.

NEXT-5 fixes the build-cost tool's obsolete `public/wasm/web_bg.wasm` assumption.
It follows `index.html` to its shipped bootstrap and the bootstrap's explicit wasm
reference, rejecting missing/ambiguous paths before building. Reports include
exact artifact paths and SHA-256 hashes, so stale fingerprinted assets cannot
silently become the measurement.

The NEXT-4 release-web frame run is **20/21 strict**, not an accepted matrix:
preview-on Rust scrollbar dragging p95/max 33.335 ms missed 16.7 ms. Other cases
passed. Raw `target/next4-fps-web/editor-fps.json` retains every frame; the bench
lock and both heavy slots were reserved. It preceded only the final keyboard
link/image follow addition. Earlier 21/21 WP-02 evidence is a separate run; do not
replace this failed case with that result. Physical desktop frames remain open.

## Final isolated build and size results

Report: `target/next5-build-cost.json`; logs/RSS files: `target/next5-build-cost-logs/`, orchestration `/tmp/moonkale-next5-build-cost-final.log`. Bench lock and both heavy slots held through every phase and size capture. Worktree-local fresh target `target/next5-isolated-native`; jobs=2, release, locked/offline; downloaded dependencies and warm filesystem/compiler caches are allowed. Report HEAD is base `6cd65e30` plus the uncommitted NEXT-5 changes. The initial product attempt fails at the upstream Ladybug registry metadata write/source fallback (P-199); the identical normal retry is separately measured in `target/next5-desktop-retry.json`, with compiler output `target/next5-desktop-retry.log`. No failed time is substituted or relabeled as success.

| phase | wall seconds | max compiler/linker RSS (KiB) | exit |
| --- | ---: | ---: | ---: |
| Fresh fixture target | 207.450 | 1,423,244 | 0 |
| No-change fixture | 0.206 | 128,020 | 0 |
| Application-only fixture rebuild | 4.679 | 680,908 | 0 |
| Desktop theme seed (cached target) | 0.381 | 197,320 | 0 |
| Actual desktop initial attempt (FAILED) | 1108.119 | 2,566,292 | 101 |
| Actual desktop identical retry (existing target) | 161.730 | 1,971,796 | 0 |

RSS is the maximum compiler/linker child process, not application RSS. The actual desktop phase uses its existing partially cached product target; it is not a clean-product build. The fixture fresh/no-change/application phases share one profile/features/target. `DOCS_RS=1` skips only theme generation for the fixture and separate dioxus-code seed; it is removed for the normal actual desktop build. Native database compilation/linking still runs. The first product attempt installed the pinned prebuilt archive in its Cargo OUT_DIR, then its registry-local metadata write failed and triggered a source fallback needing absent CMake. The unchanged retry accepts that complete archive before invoking the downloader and preserves configured symbol localization (P-199). No registry source or permissions changed. This first-attempt/retry behavior remains an upstream issue, not a clean-product build pass.

| artifact | raw bytes | gzip bytes |
| --- | ---: | ---: |
| Native production-panel fixture | 70,631,264 | 10,708,526 |
| Web server, both backends | 350,457,024 | 103,755,259 |
| Shipped main web wasm, both backends | 49,183,577 | 7,068,720 |
| CodeMirror JS bundle | 1,048,196 | 353,733 |
| Actual desktop app, both backends | 350,116,432 | 103,817,056 |

These are individual artifacts, not total package/installed/download sizes. Exact artifact paths and SHA-256 hashes are in the JSON. Main wasm is followed through the shipped HTML/bootstrap, not selected by mtime or by summing stale asset fingerprints. Both backends still ship; choosing either setting uses these same artifacts and changes build/size cost by zero. These measurements do not predict native-only retirement savings.

The freshly measured release fixture also passes GTK WebKit/AT-SPI and the visible focus-border probe: `target/next5-isolated-native-accessibility.json`, log `/tmp/moonkale-next5-isolated-a11y.log`. This independently executes the fresh/prebuilt-theme-assets artifact; the earlier final native report tested the ordinary release fixture. Both owned displays/buses are stopped.

Reproduce with fresh target/report names:

```sh
flock ~/.cache/moonkale-slots/bench.lock \
  env MOONKALE_SLOTS=1 ~/.claude/projects/-home-daniel-Code-Moonkale/scratchpad/heavy.sh \
  flock ~/.cache/moonkale-slots/2 \
  python3 packages/web/tests/e2e/editor-build-cost.py \
  --target target/next5-repeat-fresh --report target/next5-repeat-build-cost.json \
  --desktop --prebuilt-theme-assets
```

## Release-web focus decoration regression

NEXT-5 reproduced NEXT-4's failure on the unchanged release build:
`target/next5-fps-baseline/editor-fps.json` is 20/21 strict; preview scrollbar
p95/max 33.335/33.340 ms. A quiet-machine diagnostic CPU/timeline capture is in
`target/next5-fps-profile/{scrollbar.cpuprofile,scrollbar-trace.json}`. Native DOM
`replaceWith` accounts for about 955 ms of sampled self time over the gesture;
the timeline records only 83 ms layout and 51 ms style updates.

Same-build CSS removal/restoration isolates the new surface focus outline.
Changing only the selector, focus-visible matching and overlay/composited rings
all failed first-drag acceptance; those experiments are discarded. In particular,
a repeated drag must refocus the input and reset caret/viewport just as the full
matrix does. Repeating a drag without that reset can hide the initial failure.
The focus-state trace is `target/next5-fps-focus-states/scroll-focus.json`.

The final design reserves a transparent 2 px border on all four surface edges and
changes only its color on `:focus-within`. Focus cannot change layout. The surface
is excluded from the generic control outline selector; individual controls retain
their outlines. This preserves the visible source ring for both keyboard and
pointer focus, including descendant tools, without ever outlining the ancestor
of the virtual rows. The viewport has 4 px less content space in each dimension;
its existing mounted-rectangle geometry remains authoritative. Live accessibility
context, canonical selection restoration, input location and reduced motion are
unchanged. No Rust input/rendering logic is modified.

The routed-CSS diagnostic `target/next5-fps-border/editor-fps.json` passes all
seven preview gestures. Its log `/tmp/moonkale-next5-border.log` additionally
retains three independently refocused/reset scrollbar drags, all p95/max
16.670/16.670 ms. This subset is not full release acceptance. The keyboard suite
checks ring entry, exit, return and descendant hover focus; the native accessibility
probe verifies the same border in GTK WebKit.

The rebuilt **full 21-case strict release-web matrix passes 21/21**:
`target/next5-fps-final/editor-fps.json`, Chromium153.0.8010.12, no page errors.
Every p95 is <=16.670 ms; maximum frame is 33.335 ms. Preview scrollbar:
132 frames, interacted=true, p95/max 16.670/33.335 ms. The unchanged full runner
retains the input/caret/viewport reset before every gesture and all interaction
assertions. Bench lock and both heavy slots were held; bounded CPU settling
samples are in the adjacent `preflight.json`. No failed case or warm-up sample
was removed. This is web acceptance only; physical-desktop and Android remain
separate gates.

The after-change diagnostic profile is
`target/next5-fps-final-profile/{scrollbar.cpuprofile,scrollbar-trace.json}`.
It runs the same 12-case source/preview prefix through scrollbar dragging, not
the full matrix. Sampled `replaceWith` self time is about 726 ms versus 955 ms
before; timeline layout/style totals are 43/26 ms versus 83/51 ms. Scroll event
coalescing differs, so these gesture totals are not a per-event speedup claim.
The profile itself also passes that scrollbar case; only the separate unprofiled
21-case report is used for full web acceptance.

NEXT-5 repository gates pass (`/tmp/moonkale-next5-gates-final.log`): fmt,
dependency/color/string/frontmatter checks, **673 tests passed, 5 ignored**,
workspace all-target Clippy with warnings denied, minimal/Julia distribution,
public rustdoc and web wasm. The desktop fixture release build and all-target
Clippy pass too. The fresh native WebKit/AT-SPI report
`target/next5-native-accessibility-final.json` passes named input/help, polite
caret/selection text and a visible 2 px solid source border. Its private display
and buses are stopped. All **41 native-editor Chromium suites pass** on the rebuilt
fixture (`/tmp/moonkale-next5-native-accepted.log`, per-suite output under
`target/next5-native-e2e`). The earlier browser attempt overlapped a fixture
rebuild and hit a missing wasm bootstrap; it is not final acceptance evidence. No physical desktop, speech transcript or Android
acceptance is implied.

## Retirement sequence

1. **Now:** keep CodeMirror as the release default and explicit fallback. Both
   implementations, the switch UI and generated bundle remain. Test accessibility,
   migration, builds and release web frames. No setting or saved layout is erased.
2. **After Daniel's desktop and Android gates:** change `DEFAULT_CODE_EDITOR` to
   `native` and update its deliberate guard test. Unspecified preferences use Rust;
   explicit CodeMirror settings and per-document fallbacks continue to work.
   Keep the fallback for an accepted release while checking remaining consumers.
3. **After that fallback release and a zero-consumer audit:** make the core Code
   extension's stable `editor:` identity the Rust owner; retain an alias for old
   `editor-native:` layout IDs during restoration. Map legacy implementation values
   deliberately rather than discarding open documents. Markdown Source and SVG
   source views must be on the Rust path first.
4. Remove `code-view/src/backend/codemirror.rs`, CodeMirror-specific panel/bridge
   paths, `code-view/assets/codemirror.js`, `packages/js/codemirror` and its build
   scripts/dependencies only after `window.moonkale.codemirror` has no production
   consumers. Move shared toolbar/LSP/URI/edit contracts rather than deleting them.
   Convert control-only browser tests/selectors to the Rust semantic hooks; keep
   shared UTF-16 conversion, revision, source and workspace tests. Remove obsolete
   extension/switch/settings UI only after persisted IDs have an explicit mapping.
5. Re-run full platform, fidelity, accessibility, large-file, frame, size/build and
   dependency gates; update [[Code Editor]], [[Code Editor Implementations]],
   [[Extension Catalogue]], [[Status]] and crate notes. Milkdown, xterm and other JS
   views remain outside this retirement.

This is a conditional retirement plan, not a deletion or a default flip in this
change. ADR-0029 is unused; the plan implements the accepted Rust-editor direction.

## What remains for Daniel before the flip

- **Physical Linux desktop frames:** the existing 21-case CED-R22 checklist in the
  handoff, using release builds, the real GPU/session, highlighted 5000-line Rust
  with Doc preview off/on and the 3 MB line. Seven gestures per mode. Retain raw
  strict/calibrated results, inspect the earlier ~210 ms preview-wheel pause and
  confirm smooth real input. Do not substitute the headless software-GL result.
- **Android:** test the integrated shipped app after selecting Rust with the one
  setting. Open/reopen source tabs; type and select Unicode; exercise a real soft
  keyboard and Chinese IME, copy/cut/paste, undo/redo and exact save/reload. Check
  touch selection/wrap/scroll, Markdown Source/wiki behavior and accessibility
  names/focus. Missing local LSP must leave editing usable; remote-server LSP is a
  separate supported connection path. Record device/OS/app build, failures and
  saved bytes. No Android install or acceptance is claimed here.

macOS/Windows remain explicitly unverified. Optional spoken-reader review can
improve the bounded-context experience; the automated reports do not claim to
capture spoken output.

## WP-69 toolbar follow-up — 2026-10-17

Activating a command in the ⋯ menu restores its trigger when the command does not move focus to editing or a dialog. Show hover leaves that trigger focused; Tab traversal across gutter buttons preserves the popup until it can receive focus, and Escape restores the input sink. The browser keyboard journey asserts trigger restoration explicitly. Native comments use the shell’s readable hint token, and selected source uses its code foreground token, covering the actual light/dark code and selection backgrounds at the existing 4.5:1 threshold. This adds no speech-engine or physical-device acceptance claim.
