# End-to-end tests (web build)

`milestone1.mjs` drives the real app in Playwright's Firefox: open folder →
tree → open file → edit → Ctrl+S → verify on disk → conflict → reload →
second file → drag tab to split. It is the acceptance test for Milestone 1.

```sh
# one-time
mkdir -p /tmp/e2e && cd /tmp/e2e && npm init -y && npm i playwright && npx playwright install firefox

# a throwaway folder to edit
mkdir -p /tmp/m1root/src && printf 'fn main() {\n    println!("hello");\n}\n' > /tmp/m1root/src/main.rs \
  && printf '# Sample\n\nEdit me.\n' > /tmp/m1root/README.md && printf 'target/\n' > /tmp/m1root/.gitignore

# serve the web build with the folder as the allowed root
(cd packages/web && MOONKALE_ROOT=/tmp/m1root dx serve --port 8080)

# in another shell
cd /tmp/e2e && M1_ROOT=/tmp/m1root M1_SHOTS=/tmp node /path/to/packages/web/tests/e2e/milestone1.mjs
```

Screenshots land in `M1_SHOTS`. Cargo ignores `.mjs` files in `tests/`.

## Milestone 2 and 3 suites

`graph.mjs`, `links-sqlite.mjs`, `terminal.mjs`, `typst.mjs`, `lsp.mjs`, `ladybug.mjs` expect a fixture folder as `MOONKALE_ROOT` containing: `Home.md` (`See [[Alpha]] and [[notes/Beta]] and [[Missing]].`), `Alpha.md`, `notes/Beta.md`, `data.sqlite` (any table), `report.typ`, a cargo crate (`Cargo.toml` + `src/main.rs` with `fn add(a: u32, b: u32) -> u32` and a deliberate `let wrong: String = total;`), and `people.lbug` (Person/City node tables, Knows/LivesIn rel tables — see `packages/sources/graph/tests/ladybug.rs` for the Cypher). `lsp.mjs` needs `rust-analyzer` on the server's PATH.

`graph.mjs` **edits** `Home.md` (adds `[[Another]]`) — restore it before running `links-sqlite.mjs` (P-059).

## Fixture script and the full run (Milestone 6)

`fixture.sh /path/to/m2root` builds the fixture folder above deterministically (`users` table with one NULL, the small `people.lbug` via `seed_people … small`). Every suite takes `PORT` (the dev server; the desktop dev server usually holds 8080, so tests run on 8090), `M1_ROOT` and `M1_SHOTS`; `milestone1.mjs` takes `M1_URL` and needs its own root (exactly `src/` + `README.md`).

`run-all.sh` runs every suite (or the ones named on its command line), resetting `Home.md`, `.moonkale/` and generated files between them. Its working directory is **`~/.cache/moonkale-e2e`** (`MOONKALE_E2E` overrides) — deliberately not under `/tmp`, which is a tmpfs here and empties on reboot:

```sh
E=~/.cache/moonkale-e2e
mkdir -p $E/e2e && (cd $E/e2e && npm i playwright && npx playwright install firefox)   # once
packages/web/tests/e2e/fixture.sh $E/m2root                                            # once (rerun to restore)
MOONKALE_CONFIG_DIR=$E/cfg packages/extensions/wordcount/build.sh                      # once, for wasm-ext
packages/web/tests/e2e/serve.sh start        # dx serve on :8090 with $E/cfg and $E/m2root, PID in $E/dx.pid
packages/web/tests/e2e/run-all.sh            # or: run-all.sh flow phone
packages/web/tests/e2e/serve.sh stop         # kills only that dx (never `pkill dx` — it takes the desktop dev server with it)
```

Screenshots land in `$E/shots`. `milestone1.mjs` needs the server started with `serve.sh start m1root` instead.

Milestone 6 suites: `flow.mjs` (enable Flow editor + Lux in Settings → Extensions, New Flow…, place/wire/reject/Generate/Save), `wasm-ext.mjs` (wordcount module listed, enabled with `read-sources`, its command runs as an agent tool after approval), `phone.mjs` (420 px viewport: one tile, bottom bar, Editor/Graph/Terminal/Agent/Settings switching, no layout persisted, widening restores the layout). `bench.mjs` prints layout ms/step for 1k–100k nodes (a measurement, not a test); `agent-live.mjs` needs a real key.

## Milestone 7 suites

`palette.mjs` (command palette, quick open with `:line`, an extension command, menus showing bindings, rebinding through Settings), `files.mjs` (context menu: new file/folder, inline rename with an unsaved document, drag-move, delete to `.moonkale/trash`), `replace.mjs` (CodeMirror search panel; workspace replace preview and apply), `lsp2.mjs` (rust-analyzer: completion popup, `F2` rename, `Shift+F12` references, `Ctrl+.` assists), `git.mjs` (the fixture is a git repository: status, diff, stage, commit, discard, history graph), `auth.mjs` (token mode: `run-all.sh` starts its own server on port 8091 with `MOONKALE_TOKEN=e2e-secret-token`).

`serve.sh` keys its pid/log by `PORT`, so `PORT=8091 MOONKALE_TOKEN=… serve.sh start` runs next to the normal server. `run-all.sh` resets the repository (`reset --hard <root commit>` + `clean`) between suites.

## Milestone 8 suites

`history.mjs` (entity log: content/add/rename/remove/checkpoint events, active-file filter, text-at view, reload from the server's state store — read with `node:sqlite`; `run-all.sh` clears its `events` rows between suites), `presence.mjs` (two browser contexts with different names see each other on the status bar, tabs and Explorer; leaving clears), `graph3d.mjs` — **Chromium** with software WebGL (`npx playwright install chromium` once): renderer starts on WebGL2, labels drawn, the 3D toggle, orbit changes the frame. `wasm-ext.mjs` gained a step: the page is cross-origin isolated, the module ran in the browser (no `/api/ext/run` request) and keeps working with that endpoint blocked.

## Milestone 9

`duckdb.mjs` (the fixture's `data/people.csv` + `orders.csv`: open as views, join in the table editor, write refused). `history.mjs` gained restore-with-cause, `presence.mjs` the cursor gutter, `graph3d.mjs` a 3D drag. The desktop hub client has a native test instead: `MOONKALE_HUB=http://127.0.0.1:8090 cargo test -p desktop --test hub -- --ignored` against a running `serve.sh` server.

- `wiki.mjs` (spec 012): decorated links in source and rich mode, `[[` completion in both, click/Ctrl+click follows, create-on-click, Links panel *Create*, rename rewrites backlinks.
- `highlight.mjs` (spec 010): grammar tokens for Rust/Markdown/TOML, fold gutter, `Mod-/`.
- `panels.mjs` (spec 011): close/reopen static panels, side-bar collapse and return.
- `image.mjs` (spec 008): png/svg open in the viewer, zoom states, SVG Source.
- `shell.mjs` (spec 009): activity bar, badges, hide/show, Ctrl+B, registry-built menus, source row decoration.
- `android-cdp.mjs` / `android-pinch.mjs`: not suites — helpers for the phone over `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>` (evaluate JS; inject a pinch and a rotation, spec 006).
- `graph3d.mjs` gained a two-finger step (CDP `Input.dispatchTouchEvent`).

## Milestone 16

- `watch.mjs`: files written, changed and deleted in the fixture behind the app's back appear in Sources (and the search index) unprompted; a new directory is watched; hidden and `.gitignore`d files never show; a database source has ↻ and the context menu offers Refresh.
- `graph-local.mjs` (**Chromium**, software WebGL like `graph3d.mjs`): Local lays the neighbourhood out fresh; back to Whole restores the camera and node positions exactly.
- `touch-drag.mjs` (**Chromium**, touch emulation at 420 px, touches via CDP `Input.dispatchTouchEvent`): a long press on a tab drags it to a dock zone and splits the phone tile; a swipe does not drag, a tap still activates.
- `rich.mjs` gained a step: a file with `<br />` lines opens clean, and the next save writes none.

Gotchas found writing them: the activity-bar entry `#mk-rail-explorer` *toggles* (click it only when `.mk-explorer` is hidden); a narrow side tile squeezes tab titles ("Sour…"); a swipe scrolls the tab strip, so `scrollIntoView` a tab before touching it.


### Production Rust engine fixture

`native-editor-workspace.mjs` uses the actual RustCodeEditorPanel and an in-memory Source through Workspace. It checks save/reload, Unicode selection, history/remount, duplicate views, composition, clipboard, CRLF, reveal and bounded long-line rendering without the full shell/server build.

```sh
cd packages/web/tests/fixtures/native-editor
dx serve --platform web --port 8097
# In the Playwright environment:
PORT=8097 node /path/to/native-editor-workspace.mjs
```

Use `code-native.mjs` with `M1_ROOT` pointing to a disposable full-shell folder fixture to check the real folder source and layout/editor switching. The Rust sink textarea carries only input fragments; tests must assert engine/Workspace state or saved source text, never read its value as the document.

The production Rust fixture also checks shared wrap/indentation controls, CRLF indentation, settings changes without document history entries, horizontal scrolling/editing of an unwrapped 3 MB line and remount retention. `codemirror-preferences.mjs` independently checks the bundled CodeMirror preference bridge with Playwright; run it from the repository root or pass `CM_BUNDLE` with the absolute bundle path.

The production Rust fixture also covers grammar-based HTML, Markdown and Julia folding: nested gutter controls, parent/child collapse retention across remount, search into hidden content, Unicode/CRLF edits and undo. These run in `native-editor-workspace.mjs` with the same fixture server.

The Rust fixture exercises syntax-aware Enter on commented Rust/Python headers, Julia keyword headers and HTML tag pairs. It checks ignored string punctuation, caret placement through subsequent typing, exact CRLF text and separate newline/typing undo entries.

Closing-token flows cover leading bracket alignment, skipping an existing closer, literal input/paste, nested Julia `end` and HTML end tags, caret placement, and one-edit CRLF undo/redo.

Lean folding coverage checks namespace/section/declaration/nested proof controls, sibling visibility, remount retention, search through hidden proof lines and Unicode/CRLF edit undo/redo. Additional-panel checks release the large primary fixture after completing its performance assertions.

`native-editor-diagnostics.mjs` uses the same Rust editor fixture with a deterministic JSON-RPC language server. It checks Unicode diagnostic marks, zero-width gutter markers, message reveal, publication clearing, stale versions, shared duplicate-view open/close lifecycle, remount, edit/undo synchronization and save notifications. Run it with `PORT` pointing to the fixture server; no rust-analyzer installation is required.

`native-editor-hover.mjs` uses that deterministic server for pointer/caret hover, Unicode/CRLF request coordinates, keyboard activation and dismissal, debounce, canceled/out-of-order responses, edit/unmount invalidation, empty/error results and the five-second timeout. Run it against the same fixture with `PORT`; hover text is deliberately plain text and escaped.

`native-editor-completion.mjs` covers the production panel's completion shortcut/toolbar, automatic suggestions, sorting/details, keyboard/mouse acceptance, long-list scrolling, explicit Unicode/CRLF edits, snippets flattened to text, atomic additional imports and undo/redo, cancellation/stale replies, cursor/edit/unmount invalidation and timeout. It uses the same deterministic fixture and `PORT`.

`native-editor-definition.mjs` covers F12/toolbar navigation, freshly mounted caret, Unicode/CRLF positions, folded targets, encoded cross-file paths, originating-folder resolution, dirty targets, LocationLink selection ranges, empty/error/missing/outside targets, sibling panes, Escape/caret/edit/tab/unmount cancellation, canceled file loading and timeout recovery. Run with `PORT` against the same deterministic fixture.

`native-editor-rename.mjs` checks F2/toolbar prompt focus, blank/unchanged-name validation, Escape, Unicode/CRLF positions and caret rebasing, grouped undo/redo, document versions, invalid/overlapping/outside/malformed/missing edits, all-target rejection, closed/dirty cross-file targets, unsaved target synchronization/close, cursor/edit/tab/other-file/unmount cancellation, canceled loading, timeout recovery and completion with retained document leases. Run with `PORT` against the same deterministic fixture.

`native-editor-tools.mjs` covers Mod-./toolbar code actions, backward UTF-16 selection ranges and diagnostic context, preferred/disabled/escaped rows, keyboard/mouse selection and scrolling, lazy resolve/metadata, malformed responses, declared edits with ignored commands, atomic multi-file edits and stale unmounted sessions, Shift-F12/toolbar references, deduplication, decoded filenames and folder boundaries, folded/dirty targets, empty/error results, stale replies, cursor/edit/tab/other-file/unmount cancellation, guarded target loading, request/resolve timeout recovery and switching-tool focus. Run with `PORT` against the same deterministic fixture.

`native-editor-review.mjs` exercises dock tile resizing, wrapping-width updates, measured row height, scroll geometry and document switching with externally reset retained models against the native fixture. Run it with the same `PORT` setting.

`native-editor-presence.mjs` checks live native gutter initials, current-window/other-file filtering, cursor movement, remount retention and departure. It uses `presence`, `presence-move` and `presence-clear` fixture controls; run against the same native fixture with `PORT`.

`native-editor-wiki.mjs` checks native Markdown link marks, emoji/CRLF offsets, local edit remapping, index refresh, completion without LSP, existing closing brackets, grouped undo, modifier-click navigation, source identity, missing-page creation and delayed reply cancellation. It uses the fixture wiki index and `wiki`, `wiki-completion`, `wiki-code`, `wiki-index-toggle`, `wiki-delay` and `wiki-release` controls with the same `PORT` setting.

`native-editor-integration.mjs` checks workspace API reveals, Unicode/CRLF cursor and word context, scroll visibility, retained tab context, dock movement, mounted/unmounted replacement and reveal, workspace replacement/reload and close/reopen ordering. `native-editor-windows.mjs` connects two browser pages with a fixture BroadcastChannel session bus, moves a saved native tab, refuses dirty offers and preserves edits made after an offer. Both use the native fixture's `integration-*` controls and the same `PORT`. The fixture keeps document identities fixed across close/reopen. These test browser transport/API integration, not native OS drag gestures or concurrent-source save conflicts.

The same native-editor fixture has an opt-in `desktop` feature. Build with
`--no-default-features --features desktop`; direct cargo launches embed the
production styles. `native-editor-desktop.py` checks actual OS typing, keyboard
selection, forward/backward pointer dragging, Unicode clipboard, history and resized input against the optional
`MOONKALE_NATIVE_REPORT` Workspace report. Dragging verifies replacement and undo while cursor-word context changes; the browser review suite asserts that the source cells remain at the same height. The earlier native baseline used WebKit renderer overrides; the isolated default-configuration DOM probe now passes, while its current optional OS retry stops at mapped-window detection;
this does not complete platform, IME or accessibility acceptance. Commands and
limits are recorded in `markdown/editors/Rust Code Editor Native Acceptance.md`.

`native-editor-renderer.py PATH_TO_NATIVE_FIXTURE` launches an isolated fixture
with externally supplied renderer flags removed, verifies native DOM
focus/editing/selection/undo/resize and cleans up only its owned process. Optional
`--os-input --screenshot /tmp/renderer.png` checks target a mapped window owned by
that PID. DOM results are reported separately from OS/pixel results; no concurrent
fixture instance is allowed. The native input script rejects stale report PIDs.

For separate XWayland acceptance, add `--x11` to select GTK's backend before
launch. The default launch currently has no discoverable X window in this session;
the explicit X11 launch provides a PID-owned window and screenshot capture. Its
OS-input retry still fails at typing: fixture-only trusted-event observations
report zero pointer/key/input events despite a focused sink and a listener-ready
receipt. Failures print those observations. This does not establish real OS or
IME acceptance. The runner uses cell geometry measured after its DOM resize.
# Variable-height layout fixture

`native-editor-presentation.mjs` exercises the opt-in `#layout-presentation`
provider: hidden source, an inline label and a measured noneditable block at a
source anchor. It verifies pointer boundary mapping, active-source reveal,
selection/replacement/undo, composition and delayed/stale presentation results.
Replacements are accepted only at grapheme boundaries and wholly within one
measured run; overlapping replacements use provider/id order. The full previewed
line reveals while its caret/selection or composition is active.

Native real-input acceptance, with the desktop fixture built, is:

```sh
python3 packages/web/tests/e2e/native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --os-input --presentation --ime
```

It checks both inline-widget boundaries, source reveal, typing/undo, canonical
markup copy and real Pinyin source preservation/single commit/undo. Clipboard
and active input method are restored. The fixture report uses a receipt protocol
to avoid the native evaluator lifetime race. This is a synthetic bounded fixture;
Markdown providers and general cross-run widgets remain deferred.

`native-editor-proportional-unicode.mjs` uses `#layout-unicode-document` to cover
combining accents, skin-tone/ZWJ emoji, regional-indicator flags, Hangul, Indic
text, CJK and tabs. It checks whole-grapheme source runs, pointer/keyboard hits,
selection replacement/undo, CSS-row Home/End, short and empty uniform rows,
retained pixel columns and reflow at 340/670/950px editor widths. Run it against
the standalone web fixture on port 8099 (or set `PORT`). The vendored engine now
wraps at grapheme boundaries in character and word modes; an oversized cluster
stays on a nonempty row. Headless coverage checks widths 1–24 and indentation.

Measured-wrap OS acceptance uses the same owned-process runner:

```sh
python3 packages/web/tests/e2e/native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --os-input --proportional
```

The runner initializes the fixture through DOM controls, then
`native-editor-proportional-os.py` sends real X input and observes canonical
Workspace text and source/caret geometry. It checks visual Home/End, same-offset
wrap affinity, Up/Down, selection replacement, typing and undo. This suite has
passed on XWayland; retries also exposed intermittent missing OS keystrokes and
geometry-readiness failures. Trusted-event traces include keys, modifiers and
bounded source observations; failure output includes the native runtime log.

Add `--ime` to check configured Fcitx5 Pinyin using its GTK module. The check
requires unchanged canonical text during preedit, one Chinese commit and undo.
The previous active input method is restored, including runner timeout cleanup.
Current IME attempts fail at preedit; this gate remains open. Supply the active
desktop's display/session environment when launching from a terminal session
without it; stale X authorization or an incorrect session type can prevent setup.

The later isolation run reproduced and fixed two causes: native eval replies
could be lost when the geometry evaluator finished before Rust consumed them,
and WebKit's `input`-then-`compositionend` sequence could insert the same IME
candidate twice. Geometry now uses a receipt acknowledgement; consumed sink
generations reject the trailing compositionend. The measured OS/Pinyin suite
passes with one `你好` commit and Workspace undo. This WebKit/Fcitx path also
omits compositionstart/update in a plain visible textarea, so their absence is
recorded rather than treated as an editor-specific preedit failure.

`--x11 --os-input --isolate` compares that plain textarea with the editor and
reports per-run measurement status, direct DOM rectangles and trusted events.
It restores the active input method and requires a single editor commit and
undo when Pinyin input is delivered. A zero-event run is reported as an input
delivery limitation, not accepted as an IME test.


Run `native-editor-layout.mjs` against the standalone native-editor web fixture on port 8099 (or set `PORT`). It exercises the feature-gated taller row and anchored block through the production surface, including height-change scroll anchoring, remount, reveal, editing/undo, resize and folding. It also checks DOM-measured block heights, delayed stale measurements, fractional remount restoration and source-line anchoring across rewrap. This does not verify real native IME.

`native-editor-decorations.mjs` runs against the same fixture and verifies overlapping Unicode search/diagnostic and search/wiki decorations, tooltip precedence and unchanged source. `native-editor-workspace.mjs`, `native-editor-diagnostics.mjs` and `native-editor-wiki.mjs` retain the broader provider lifecycle and editing checks. Set `PORT=8099` for suites whose historical default differs.

`native-editor-proportional.mjs` opens the opt-in proportional geometry probe and tests pixel wrapping, Unicode/grapheme/tab hits, Rust selection/editing/undo and stale measurements. It uses port 8099 by default. Native WebKit DOM/layout verification is available after building the desktop fixture:

```sh
python3 packages/web/tests/e2e/native-editor-proportional.py target/debug/moonkale-native-editor-host
```

The native runner requires no other native fixture process, removes external renderer overrides, and cleans up its own child. Its synthetic DOM input is separate from OS/IME acceptance. The production code view does not enable proportional rendering yet.


`native-editor-proportional-integrated.mjs` checks the opt-in measured source run inside the shared Workspace surface: scalar decorations, indexed row heights, bounded virtualization and edit/undo. Run against the standalone native-editor web fixture on `PORT` (default 8099). This fixture is limited to one wrapped logical line of at most 4096 scalars; it does not enable production proportional layout or Markdown preview.


`native-editor-proportional-navigation.mjs` uses the same fixture to check measured Unicode Up/Down, retained pixel columns, Shift selection/replacement/undo and synthetic composition. Real OS/IME acceptance remains separate. The native proportional DOM probe also exercises measured Up/Down.


`native-editor-proportional-edges.mjs` checks visual Home/End, caret wrap affinity, boundary Left/Right, Shift+End replacement/undo and mixed measured/ordinary pixel-column movement. It uses the same feature-only fixture and `PORT` convention. The native proportional DOM driver also verifies visual edges and affinity; OS/IME remains a separate gate.


`native-editor-proportional-fonts.mjs` checks delayed/failed/preloaded font invalidation, stale targets, caret/resize stability and watcher cleanup. It uses existing KaTeX Typewriter/Main WOFF2 assets offline. The default font path resolves from the repository; when copying the script elsewhere, set `MOONKALE_TEST_FONT` to `packages/editors/markdown/assets/katex/fonts/KaTeX_Typewriter-Regular.woff2` (the Main font must be beside it). The native proportional driver also verifies a loaded local monospace face and refreshed pointer geometry.

Cross-run presentation coverage: `native-editor-presentation.mjs` forces both hidden and inline tokens across engine run boundaries at multiple widths, checks unique ownership and complete visible source order, then verifies boundary hits, source reveal, selection, undo, composition and stale results. Native `--x11 --os-input --presentation --ime` covers canonical clipboard copy and real Pinyin commit/undo using the longer fixture source.

Consumed-run coverage: `native-editor-consumed-runs.mjs` verifies zero spacing and restoration of several fully replaced source runs. Native runner `--presentation --consumed-runs` checks complete source ownership and zero gaps; add `--x11 --os-input --ime` to test full canonical copy, pointer boundaries, undo and Pinyin input.

Local reveal coverage: `native-editor-local-reveal.mjs` checks independent constructs, local and crossing selections, resizing, IME commit/undo and stale reveal masks. Presentation/consumed-row suites now expect unrelated constructs to stay in preview. Native `--x11 --os-input --presentation --consumed-runs --ime` enters the widget for Pinyin input and verifies the other source range remains hidden while canonical copy and undo remain exact.

Multiline coverage: `native-editor-multiline.mjs` tests canonical newline ownership, empty-line collapse/reveal, widget endpoints at several widths, local reveal, selection, navigation, undo, composition, scrolling and stale masks. Native `--x11 --os-input --presentation --multiline --ime` verifies full canonical clipboard copy and Pinyin commit/undo inside a revealed multiline widget. `--multiline` excludes `--consumed-runs`.

`native-editor-review-2.mjs` checks Claude's second-review regressions against the standalone fixture (`PORT=8099`): differently encoded diagnostic URIs, mounted Rust/CodeMirror recovery after server closure, stale server-version rejection, atomic multi-file CodeMirror edits, canonical CRLF preservation and undo. `native-editor-tools.mjs` also verifies original diagnostic code/source/data/unknown fields and that valid code actions survive malformed siblings.

`native-editor-widgets.mjs` checks interactive block content against the standalone editor fixture (`PORT=8099`): focus/input/clipboard isolation, expansion, view-local state, duplicate panes, source return, undo, natural height, delayed measurements, resize and scrolling. Native acceptance is `python3 packages/web/tests/e2e/native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --widgets --os-input` after building the desktop fixture. This mode runs separately from proportional/presentation/IME probes and restores the editor source anchor with Escape or Edit source.
