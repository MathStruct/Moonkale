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
Measured results and exact reproduction paths follow below after the run.

The NEXT-4 release-web frame run is **20/21 strict**, not an accepted matrix:
preview-on Rust scrollbar dragging p95/max 33.335 ms missed 16.7 ms. Other cases
passed. Raw `target/next4-fps-web/editor-fps.json` retains every frame; the bench
lock and both heavy slots were reserved. It preceded only the final keyboard
link/image follow addition. Earlier 21/21 WP-02 evidence is a separate run; do not
replace this failed case with that result. Physical desktop frames remain open.

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
