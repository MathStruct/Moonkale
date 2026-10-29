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

`lsp.mjs` and `lsp2.mjs` share the engine adapter in `lsp-editor.mjs`.
The default is CodeMirror; run the same scenarios on Rust with:

```sh
LSP_EDITOR=native MOONKALE_E2E=… PORT=… packages/web/tests/e2e/run-all.sh lsp lsp2
# Control run, with the fixture reset by run-all between suites:
LSP_EDITOR=codemirror MOONKALE_E2E=… PORT=… packages/web/tests/e2e/run-all.sh lsp lsp2
```

Both use disposable `M1_ROOT/src/main.rs`, add an emoji/CJK comment before the
call and convert the fixture to CRLF. They observe actual websocket hover and
definition positions, then verify diagnostics, completion, rename/stale prompt,
references and edit-based code actions through the UI. Editor text comes from
visible source rows; neither suite reads the Rust input sink as document text.
Edits and cleanup use narrow keyboard selections, avoiding browser-specific
multiline `insertText` behavior. Undo cleanup allows each engine's own typing
transaction grouping. The adapter is for these small fixtures, not a
whole-document reader for virtualized large files.

The standalone production panel can also use live rust-analyzer in native WebKit:

```sh
CARGO_TARGET_DIR="$PWD/packages/web/tests/fixtures/native-editor/target" \
  cargo build --manifest-path packages/web/tests/fixtures/native-editor/Cargo.toml \
  --no-default-features --features desktop
python3 packages/web/tests/e2e/native-editor-live-lsp.py \
  packages/web/tests/fixtures/native-editor/target/debug/moonkale-native-editor-host \
  --report target/native-live-lsp-result.json
```

Use `heavy.sh` for the build and runner. The runner creates a private display and
Cargo corpus under this worktree's `target/native-live-lsp`, keeps rust-analyzer's
build output there, rejects an existing report/display and stops its owned display
on exit. The fixture discovers the installed server through the existing local
service when `MOONKALE_LSP_ROOT` is set; ordinary browser/native fixtures retain
their deterministic server. The result records all seven LSP features, original
and final canonical CRLF text and WebKit metadata. Edits remain unsaved. DOM input
checks the native adapter and live-server integration; it does not accept physical
OS input, IME, accessibility, other platforms or CED-R22 frame timing.

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


`native-editor-proportional-fonts.mjs` checks delayed/failed/preloaded font invalidation, stale targets, caret/resize stability and watcher cleanup. It uses existing KaTeX Typewriter/Main WOFF2 assets offline. The default font path resolves from the repository; when copying the script elsewhere, set `MOONKALE_TEST_FONT` to `packages/ext-api/assets/katex/fonts/KaTeX_Typewriter-Regular.woff2` (the Main font must be beside it). The native proportional driver also verifies a loaded local monospace face and refreshed pointer geometry.

Cross-run presentation coverage: `native-editor-presentation.mjs` forces both hidden and inline tokens across engine run boundaries at multiple widths, checks unique ownership and complete visible source order, then verifies boundary hits, source reveal, selection, undo, composition and stale results. Native `--x11 --os-input --presentation --ime` covers canonical clipboard copy and real Pinyin commit/undo using the longer fixture source.

Consumed-run coverage: `native-editor-consumed-runs.mjs` verifies zero spacing and restoration of several fully replaced source runs. Native runner `--presentation --consumed-runs` checks complete source ownership and zero gaps; add `--x11 --os-input --ime` to test full canonical copy, pointer boundaries, undo and Pinyin input.

Local reveal coverage: `native-editor-local-reveal.mjs` checks independent constructs, local and crossing selections, resizing, IME commit/undo and stale reveal masks. Presentation/consumed-row suites now expect unrelated constructs to stay in preview. Native `--x11 --os-input --presentation --consumed-runs --ime` enters the widget for Pinyin input and verifies the other source range remains hidden while canonical copy and undo remain exact.

Multiline coverage: `native-editor-multiline.mjs` tests canonical newline ownership, empty-line collapse/reveal, widget endpoints at several widths, local reveal, selection, navigation, undo, composition, scrolling and stale masks. Native `--x11 --os-input --presentation --multiline --ime` verifies full canonical clipboard copy and Pinyin commit/undo inside a revealed multiline widget. `--multiline` excludes `--consumed-runs`.

`native-editor-review-2.mjs` checks Claude's second-review regressions against the standalone fixture (`PORT=8099`): differently encoded diagnostic URIs, mounted Rust/CodeMirror recovery after server closure, stale server-version rejection, atomic multi-file CodeMirror edits, canonical CRLF preservation and undo. `native-editor-tools.mjs` also verifies original diagnostic code/source/data/unknown fields and that valid code actions survive malformed siblings.

`native-editor-widgets.mjs` checks interactive block content against the standalone editor fixture (`PORT=8099`): focus/input/clipboard isolation, expansion, view-local state, duplicate panes, source return, undo, natural height, delayed measurements, resize and scrolling. Native acceptance is `python3 packages/web/tests/e2e/native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --widgets --os-input` after building the desktop fixture. This mode runs separately from proportional/presentation/IME probes and restores the editor source anchor with Escape or Edit source.


`native-editor-markdown.mjs` checks the opt-in Markdown provider (`PORT=8099`): italic/bold/code labels, decoded entities and Unicode source endpoints across three widths, local reveal, canonical source copy (existing LF clipboard convention), CRLF edit/undo, synthetic composition and delayed provider disable. Parser/model tests cover unsupported syntax, uncapped constructs, changed-block reuse, visible-window source ownership and provider identity. Native `native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --markdown --os-input` verifies WebKit styling, real pointer entry, canonical source typing/undo and preview restoration. Markdown mode runs separately from other modes; real OS IME is currently covered by the shared presentation pipeline with `--presentation --os-input --ime`, not by the Markdown probe.


The Markdown browser suite also resets all layout fixture flags and uses the production Preview toolbar/settings path, verifies pressed state and wrap behavior, remount persistence, source retention and non-Markdown isolation. The code-view crate defaults to the `markdown-preview` compile-time feature, while runtime `editor.markdown_preview` defaults off. `layout-fixture` is needed only for the fixture controls/probe exports. Default-build unit coverage explicitly supplies no fixture context.


`native-editor-markdown-nested.mjs` verifies combined/nested emphasis, strong and inline-code fragments through the production Preview toggle: correct per-fragment CSS inheritance, one outer source target, local reveal, pointer endpoints at three widths, canonical copy, CRLF edit/undo and composition. Native `native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --markdown --nested --os-input` exercises the same nested source with real pointer/typing/undo. `--nested` requires `--markdown`; the ordinary Markdown mode remains available.
## E3: markdown round-trip fidelity (2026-10-11)

`roundtrip.mjs` is experiment E3 of the editor-extensibility RFC (see the review and [[ADR-0019 Extension coordinates and block addressing]]). For each file in `corpus/` and each mode (Source = CodeMirror, Rich = Milkdown), it does two things:
1. It opens the file. The tab must stay clean and the file unchanged.
2. It appends ` EDIT` to the file's `Sentinel paragraph.`, which sits in the middle of the file, and saves. The file must then equal the original plus exactly that edit.

It copies the corpus to `<root>/e3/<mode>/` and opens each file through quick open in a fresh page. It prints a table and writes `$M1_SHOTS/e3-roundtrip.{json,md}`.

It fails on harness errors, and on fidelity failures in the modes listed in `E3_STRICT`: by default `source`, which must stay byte-exact; Rich is measured only (`E3_STRICT=` measures everything, `E3_STRICT=source,rich` gates both). `E3_MODES=rich` and `E3_ONLY=crlf` narrow a run. A file that hangs the page (P-164) counts as an error after `E3_HANG_S` seconds (default 90), and the suite continues with a new browser. It needs only the corpus, not the full fixture: a root with `.moonkale/settings.json` = `{"editor":{"markdown_rich":false}}` is enough.

The corpus covers inline markup, lists, tables, wiki and markdown links, callouts and `^block-ids`, math, code fences, front matter, pasted HTML, Unicode (ZWJ emoji, combining marks, CJK, RTL, bidi controls), CRLF, a missing final newline, a 20 000-character line, a BOM, images and a leading thematic break. `corpus/.gitattributes` says `-text`, so git never normalises the line endings the corpus exists to test. Add a file by writing it with a `Sentinel paragraph.` paragraph somewhere in the middle.

First run (2026-10-11, `dev-claude`): **Source 15/16 exact**; the 16th, `11-crlf.md`, hangs the tab on the first keystroke (P-164). **Rich 2/16 exact** (Unicode and the long line). Milkdown:
- marks six files dirty on open (lists, tables, links, callouts, code, CRLF);
- rewrites setext headings as ATX, `-` bullets as `*`, `---` breaks as `***`, and pads table columns;
- inlines reference links and images, and drops a fence's info string after the language (`title="main.rs"`);
- escapes callouts (`> \[!note]`) and turns `$$…$$` on one line into `$…$`;
- drops inline `<br>`, the BOM and CRLF, and adds a final newline.

After the P-164 fix (same day): **Source 16/16**, now strict by default. P-165 fixed: Rich mode no longer marks any corpus file dirty on open (still 2/16 exact on save: the rewrites listed above remain).

`codemirror-crlf.mjs` checks LF and CRLF canonical UTF-16 selection callbacks, Unicode splice coordinates and undo against the bundled view. Reconciliation with P-164 preserves the view separator and removes the former Rust LF mirror/CRLF expansion; `roundtrip.mjs` Source remains 16/16 byte-exact.


`native-editor-markdown-headings.mjs` checks all six ATX levels through the production Preview toggle: mixed inline fragments, full source-marker ownership, pointer boundaries at three widths, retained variable row heights without gaps, local reveal, canonical source copy, CRLF editing/undo and synthetic composition. Native `native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --markdown --headings --os-input` covers WebKit styles and real pointer/source input/undo. `--headings` requires `--markdown` and excludes `--nested`; its enlarged viewport keeps all heading targets mounted.


`native-editor-markdown-setext.mjs` covers two-line Setext headings: levels one/two and inline formatting, collapsed underline rows versus retained blank rows, whole-source boundary hits at three widths, caret/selection on the underline, source copy, CRLF edits/undo, composition and delayed provider disable. Native `native-editor-renderer.py target/debug/moonkale-native-editor-host --x11 --markdown --setext --os-input` verifies the multiline canonical token through real pointer/source input/undo. `--setext` requires `--markdown` and excludes `--nested` and `--headings`.


`native-editor-markdown-fenced.mjs` checks closed code-block previews through the production Preview toggle: Rust syntax classes/theme colors, unknown-language plain text, literal Markdown/HTML safety, source endpoints at three widths, consumed fence/body rows, retained blank rows, code-body caret/selection, canonical copy, CRLF edits/undo, composition, partially clipped scrolling and stale disable. Native fenced acceptance also checks actual theme colors and therefore needs a fixture built with its assets using `dx build`, rather than a plain Cargo binary. It verifies the same full block source token with real pointer/keyboard input. `--fenced` requires `--markdown` and excludes other Markdown fixture modifiers. Earlier inline fixtures now use indented code as their unsupported literal-code example, since closed root fences are supported.


Build and run the native fenced probe with bundled theme assets (Linux, default fixture target directory):

```sh
(cd packages/web/tests/fixtures/native-editor && CARGO_NET_OFFLINE=true dx build --platform desktop --no-default-features --features desktop)
python3 packages/web/tests/e2e/native-editor-renderer.py packages/web/tests/fixtures/native-editor/target/dx/moonkale-native-editor-host/debug/linux/app/moonkale-native-editor-host --x11 --markdown --fenced --os-input
```

The standalone Cargo fixture inlines the production panel/native CSS for earlier geometry probes, but cannot supply hashed syntax theme assets. The Dioxus-built fixture bundles those styles; fenced acceptance fails with stylesheet-load diagnostics if theme colors are missing. The displayed binary path assumes no `CARGO_TARGET_DIR` override.

## Spec 031 / ADR-0020: the ModelingToolkit library (2026-10-12)

- `rxinfer.mjs`: enable Flow editor + the RxInfer library, build a coin's bias from the palette — θ ~ Beta(2, 7) and ten flips y ~ Bernoulli(θ), the flips typed into the variable's `data` in the floating parameter editor — refuse a factor–factor wire and a second variable on one channel (the bipartite rule, ADR-0022), generate `inference.jl` **byte-exact against the golden**, and run it with `julia --project=$RX_ENV` when an RxInfer environment exists (default `~/.cache/moonkale-e2e/julia-rxinfer`; otherwise the step skips): seven heads in ten give the exact conjugate posterior Beta(9, 10), mean 9/19.
- `table-grid.mjs`: the table editor's virtualised grid on a 50 000-row result (a recursive CTE on the fixture's `data.sqlite`): only the window is in the DOM, the end scrolls into view, a header click sorts (descending on the second, NULLs last), the filter narrows and the meta line counts, Ctrl+End moves the keyboard cursor to the last cell and scrolls it into view, dragging a header edge widens the column without sorting, and a 60-column result draws only the columns under the viewport.
- `toolbar-icons.mjs`: ADR-0027's toolbars: every code-editor toolbar button is a symbol whose tooltip is its description plus shortcut (`Save (Ctrl+S)`), and its label is hidden text; the gear opens the extension's settings beside the panel, titled with the extension's name, holding the focus, and Escape closes it.
- `mtk.mjs`: enable Flow editor + the ModelingToolkit library (Lux off — one library per flow), place an RC low-pass from the acausal palette, edit C = 1.0 F in the floating parameter editor (τ = 1s), wire the pins — the capacitor's negative pin carries the return **and** the ground: a bus, one `connect` with all ends — refuse an electrical-onto-mechanical wire, generate `circuit.jl` **byte-exact against the golden** (sequential `@named` names, wire-order nets, uuid-suffixed block ids never emitted), and simulate it with `julia --project=$MTK_ENV` when a ModelingToolkit environment exists (default `~/.cache/moonkale-e2e/julia-mtk`; otherwise the step skips with a message): the capacitor reads `1 - e^-5 = 99.3 %` of the source after five time constants.

`rich.mjs` resolves each requested Markdown document by its path and stable `data-document-id` before clicking its own mode controls. Its retained-tab regression switches Front/Math between Source and Rich repeatedly while preserving an unsaved Front edit and leaving both files on disk intact. Readiness checks use the requested document and content; no click is forced and no transition delay is added.

For the slow Rich-button investigation, set `RICH_TRANSITION_TRACE=1` when running the suite. It samples panel/toolbar/button identity, bounds, visibility and the center hit target every 250 ms during the ordinary Front transition, capped at 160 samples, and writes `rich-Front.md-transition.json` under `M1_SHOTS`. The trace is optional; run the acceptance sequence with and without it. The two-core gate is three consecutive rounds of `files rich stores palette shell wiki`, with the owned server and browser runner both launched through `taskset -c <two available CPUs>`.

P-179 also guards asynchronous saved-tab restoration: background loads do not activate each tab, and saved focus yields to foreground document navigation. Selecting a retained document tab updates the workspace's active document. `wiki.mjs` waits for the expected selected completion option to settle for 100 ms, resetting its observation when the popup/selection/options change; this exceeds CodeMirror's intentional 75 ms keyboard interaction guard without a fixed sleep.

`live-preview-long.mjs` runs through the full shell with the native editor enabled. It creates a 247 KB CRLF vault note and a 3 MB Source baseline, scrolls through the note, checks headings 60/120/180/239, edits the final heading with Unicode, saves and undoes byte-exactly, and copies complete canonical source. Seven measured typing samples follow three warmups. The median must stay within the greater of 25 ms or 25% of the Source baseline; p95 allows 50 ms or 50%. `$M1_SHOTS/live-preview-long-latency.json` records every sample. Run on two cores with the server constrained too. These are dev-build input-to-cursor measurements, not a release-build latency guarantee.
- `flow-tree-open.mjs` (P-183): a `.flow.json` opened from the explorer tree right after the Extensions rail entry keeps its tab in front through the first edit — the shell handles each command once instead of replaying the last one when a signal it read changes.

For store-opening diagnostics, set `STORES_TRANSITION_TRACE=1`. The ordinary store-row click is sampled every 250 ms (up to 160 samples), recording row/tree/source identity, bounds and hit target in `stores-<file>-transition.json` under `M1_SHOTS`. The acceptance sequence is three consecutive two-core rounds of `files rich stores`, including runs without tracing.

The store suite also verifies explorer DOM identity when opening additional stores and reopening the folder, plus database child retention after folder and database refreshes. Ordinary store clicks capture identities synchronously on pointer-down; they do not add a browser round trip or a settling delay before clicking. Source containers and tree rows must be keyed at the direct list-child level, and refresh queries must use the node’s owning source.

`native-editor-markdown-lists.mjs` checks bullets, ordered/nested items, task status, styled fragments, wrapped item heights at narrow/wide widths, local pointer reveal, CRLF edits, task-marker undo/redo and complete source copy. Its fixture is `#markdown-lists`. The full-app `live-preview-long` note now includes three list/task items per section and checks deep lists as well as headings, with the original 25% median / 50% p95 latency bounds unchanged.


`native-editor-markdown-links.mjs` checks inline/reference/autolinks, literal Unicode wiki aliases, links inside headings/lists/tasks, wrapped glyph-specific Ctrl-click, local source reveal, CRLF edit/undo/copy, external opener capture and cancellation after an origin edit during delayed wiki lookup. `live-preview-links.mjs` exercises the full app: same-note and unsaved cross-note heading jumps, percent-encoded file paths, missing regular files, missing headings, single-suffix wiki creation and synchronous guarded external opening. The long-note acceptance includes links in all 240 sections while retaining its original latency bounds.

The link fixture keeps all 240 sections, 720 list/task items and 480 links. Plain filler decreases from 75 to 71 repeats per section to hold the note near the preceding 246 KB byte budget; the >240 KB assertion and unchanged median/p95 limits gate that comparable workload.

`native-editor-markdown-images.mjs` covers standalone source-backed Markdown/reference/wiki images, literal Unicode aliases, delayed-load removal, deduplicated bytes, metadata limits, decode failures, version changes, geometry at narrow/wide widths and CRLF edit/undo/copy. `live-preview-images.mjs` checks source-backed SVG loading and Ctrl-click asset viewing in the full app. The long-note fixture includes four image blocks while retaining its latency limits.

`native-editor-markdown-tables.mjs` covers aligned headers/cells, escaped pipes, literal Unicode wiki aliases, inline styles and modified link clicks, narrow/wide row heights, local source reveal, CRLF edits/undo/copy, composition, clipped scrolling and delayed provider disable. `live-preview-tables.mjs` verifies byte-exact CRLF save/undo/redo and copy in the full application. The long-note fixture includes three tables and checks their deep preview with the existing latency limits.

`native-editor-markdown-math.mjs` covers inline/display formulas throughout paragraphs/headings/lists/tables/link labels, macros and errors, fonts, two widths, independent equation scrolling and scrollbar hits, composition, CRLF source reveal/undo/copy, delayed renderer removal and stale disable. `live-preview-math.mjs` checks saved vault macros/source refresh and byte-exact save/undo/redo/copy. `rich` checks the shared assets. The long-note fixture retains its byte/widget budget with 240 formula link labels and three display blocks, using unchanged latency bounds.


`native-editor-markdown-callouts.mjs` covers standard/custom types, formatted and default titles, nested quotes, lists/tasks, hidden math, accessible folding without caret movement, modified links, narrow/wide row heights, local reveal, CRLF edits/undo/copy, composition and delayed preview disable. `live-preview-callouts.mjs` verifies folding and byte-exact CRLF save/undo/redo/copy through the full app. The long-note suite checks deep callouts alongside lists, equations, tables and images with its unchanged latency bounds.


`native-editor-wiki-hover.mjs` verifies wiki hover in source and rendered text (paragraphs, lists, tables, callouts), heading/same-note targets, missing notes/headings, literal alias text, narrow viewport bounds, popup focus/scroll/clipboard isolation, source bracket hits, lazy lookup cancellation, and live unsaved target edits followed by discard. `live-preview-wiki-hover.mjs` covers unopened target previews without additional tabs, unsaved target text, unchanged CRLF files and modified-click navigation in the full app. The CI smoke list includes the full-app hover suite.


`native-editor-properties.mjs` checks the expandable YAML bar, source-row collapse/reveal, draft cancellation, clipboard ownership, composition and mode-switch cleanup, atomic undo/redo, stale-draft rejection, delimiter guards, and CRLF-preserving add/remove edits. `live-preview-properties.mjs` verifies Save applies the draft, file/body bytes remain intact, undo/redo and Source/Preview switching work, and edits stay isolated between notes. The CI smoke list includes the full-app properties suite.


### Documentation preview and frame measurements

`native-editor-doc-preview.mjs` exercises the production-panel fixture in Rust, Python and Julia, including source entry, links, search, exact copy, composition, wrapping and history. `doc-preview.mjs` checks full-app CRLF save/history; `doc-preview-long.mjs` compares preview on/off on the same 263 KB Rust file and checks owners past 4096 characters, bounded rows and exact canonical copy. Run the long comparison in isolation, with both server and browser restricted to two cores; its existing relative limits remain unchanged.

`editor-fps.mjs` measures CED-R22 frame intervals for highlighted 5000-line Rust and spec 018's 3 MB single line. It records typing, repeated held arrows, wheel/small-delta scrolling, scrollbar/selection drags and caret movement, plus supported long-animation-frame/longtask entries. Reports include build kind and browser version. It is deliberately outside run-all's strict list until release web and desktop meet the requirement. A dev run cannot establish release acceptance; this browser harness does not claim desktop WebView evidence.

```sh
# Copy the script beside the installed Playwright module, as run-all does.
M1_ROOT=/path/to/disposable/fixture M1_SHOTS=/path/to/reports PORT=8094 \
  FPS_BUILD=release-web node /path/to/playwright/editor-fps.mjs
# Add FPS_STRICT=1 to fail a release web run at p95 > 16.7 ms or max > 50 ms.
```

The desktop fixture's **manual** RAF recorder uses real Wayland input. Prepare
`FpsSource.rs` and `FpsThreeMb.txt` with `editor-fps.mjs`, then build and launch:

```sh
CARGO_BUILD_JOBS=2 cargo build --release \
  --manifest-path packages/web/tests/fixtures/native-editor/Cargo.toml \
  --no-default-features --features desktop
MOONKALE_FPS_SOURCE=/path/to/FpsSource.rs \
MOONKALE_FPS_REPORT=/tmp/moonkale-desktop-fps.json \
  packages/web/tests/fixtures/native-editor/target/release/moonkale-native-editor-host
```

For each gesture, select its name, click **Record trusted input**, focus the
editor and perform it continuously for five seconds. Recording starts with the
first intended gesture. Keep the release window visible; do not run builds or
other browser suites during measurement. Record all seven gestures with Doc
preview off and on for the Rust file, then restart with `FpsThreeMb.txt` and
record the seven Source gestures. The fixture starts with Wrap off; the Doc
preview toolbar toggle is independent. Reset the caret and viewport before each
recording and use native Undo to restore typing afterward.

The report includes p50/p95/max, intervals, trusted-event counts, WebView/viewport
metadata and available long-task observations. Insufficient/no-op gestures and
development recordings cannot pass. The manual recorder injects no input. For an automated private-display run, use
`editor-fps-desktop.py --binary <release-fixture> --corpus <directory> --report <new-json-path>`.
It supplies compositor input through `tools/headless.sh`, validates all 21 cases
and stops the display on exit. See How to Write Tests §7 for bounded gestures
and the software GL limitation; real-desktop acceptance remains Daniel’s check.
The recorder is fixture instrumentation; `editor-fps-recorder.mjs` checks its
browser transport and trusted-input accounting with `MOONKALE_REPO` and `PORT`.
That check does not establish desktop frame acceptance.

The desktop recorder reports unrounded `performance.now()` callback intervals,
with the WebView's rAF deltas retained as `rafIntervals`. Integer rAF deltas alone
cannot resolve the 16.7 ms gate. The clock/gate regression runs without a browser:
`node --test packages/web/tests/e2e/editor-fps-recorder-clock.test.mjs`. It does
not establish actual WebView clock precision or desktop performance.

When GTK WebKit quantizes both clocks, the desktop report uses a calibrated
vsync fallback: at least 30 idle frames, idle p95 baseline no slower than 17 ms,
p95 ≤1.5×baseline and max ≤50 ms. It retains `strictPassed: false`, raw timings,
idle samples, missed-frame count and `gate: "calibrated-vsync"`; this does not
establish the original sub-millisecond 16.7 ms desktop gate. The driver waits
1.2 seconds after arming for calibration. Fractional clocks keep the original
gate. Acquire `~/.cache/moonkale-slots/bench.lock` and both heavy slots before
timing runs, and run no builds or other suites concurrently.

Use `editor-fps-desktop.py --gesture wheel` (or another gesture) for a three-case
diagnostic subset. The summary records `expectedCases` and `completeMatrix`;
a subset cannot establish all 21 cases. `HEADLESS_GL_THREADS=0..32` optionally
controls Mesa render workers for profiling and is recorded in the summary; the
default remains Mesa’s own choice. Missing recorder results retain a screenshot.

Desktop FPS preflight samples aggregate `/proc/stat` activity before each mode and gesture. It requires two consecutive samples at or below 10% active CPU within six attempts; all samples are retained in the summary. Settling occurs before input, never by discarding a failed gesture. Hold the bench lock and both heavy slots throughout the run.


### Rust editor accessibility, migration and build costs (WP-03 M5)

`native-editor-accessibility.mjs` runs the production-panel fixture's keyboard
journey, actual Chromium accessibility tree, focus/reduced-motion checks and
source contrast using the production theme palette (`MOONKALE_REPO` required).
`native-editor-wiki.mjs` also checks primary-Enter navigation. Run these with the
other native fixture suites. `editor-migration.mjs` uses the ordinary full app and
disposable fixture (`src/main.rs` and `second.txt`): backend selection alone enables
Rust, live dirty buffers/active Unicode selection survive, per-file fallback
outranks global changes and saved tabs restore after reopening the folder.

The native accessibility runner needs the installed Python GI Atspi typelib,
`dbus-run-session` and the usual headless dependencies:

```sh
python3 packages/web/tests/e2e/native-editor-accessibility.py \
  packages/web/tests/fixtures/native-editor/target/debug/moonkale-native-editor-host \
  --report target/native-accessibility-result.json
```

Use heavy.sh. It creates a private session/accessibility bus and short temporary
runtime sockets, with builds and retained reports/logs in the worktree. It reads
only its own AT-SPI tree, verifies the input/help and canonical status text plus
`live=polite`, and stops its display. No physical input or spoken-reader claim.

Build-cost measurement (fresh worktree-local target, already downloaded packages):

```sh
flock ~/.cache/moonkale-slots/bench.lock \
  env MOONKALE_SLOTS=1 ~/.claude/projects/-home-daniel-Code-Moonkale/scratchpad/heavy.sh \
  flock ~/.cache/moonkale-slots/2 \
  python3 packages/web/tests/e2e/editor-build-cost.py \
  --target target/editor-clean-measurement --report target/editor-build-cost.json --desktop
```

If the registry is read-only but its generated dioxus-code theme CSS already
exists, add `--prebuilt-theme-assets`. This uses upstream's `DOCS_RS=1` path to
skip regenerating registry assets during the fixture build. For the desktop target it first seeds only `cargo build -p dioxus-code` in that
mode, then explicitly removes the variable for the actual product build, so native
database dependencies still build/link. The two phases are reported separately.
A missing generated CSS asset is still an error; no source permissions change.

This reserves both heavy slots without sharing another worktree's target. It
records fresh-target, no-change and application-only rebuild times/compiler RSS
under the same release profile, then raw/gzip artifact sizes. The main web wasm
is resolved from the shipped HTML/bootstrap rather than a fixed path or the newest
file in an accumulated assets directory; paths and SHA-256 hashes identify the
measured artifacts. Build the release web bundle first. `--desktop` also builds
the actual desktop app against this worktree's existing dependency target. It does not claim
cold compiler/filesystem caches, application RSS or size savings from changing a
default while both backends still ship. See the vault's Rust Code Editor
Accessibility and Cutover page for the retirement sequence and remaining gates.

### Experimental Rust graph edits

Build both halves with `SERVE_FEATURES=julia,structural-edits` and run
`run-all.sh structural-edits`. This suite requires Chromium's software GL (as
`graph-a11y` does). It covers real graph selection, remote planning, safe source
preview/discard, unsaved Apply, native undo/redo/save, move/conflict/recompute,
and endpoint bounds. Repeat with `STRUCTURAL_EDITOR=codemirror` to cover the
CodeMirror mirror/undo/save path. CI runs both backends. The default all-suites list excludes this opt-in feature.

`graph-rename` requires `structural-edits` on server/client and rust-analyzer. It drives reference-aware graph Rename with unsaved calls, recursion, shadowed locals, exact comments/CRLF, grouped undo/redo, complete two-file preview/apply, separate saves, newer-edit blocking, history from either document and retained conflicts. Repeat with `STRUCTURAL_EDITOR=codemirror`; default is native.

### Startup recovery and evidence

`serve.sh` launches dx and its descendants in an isolated process group. Python 3
records its PID birth identity; `stop` verifies that identity before signalling
the group, so orphaned compiler/server children are included without matching
other sessions by name. A legacy PID file without an ownership record, or a
mismatched birth identity, is refused and preserved for manual inspection. Do not remove the `dx-PORT.owner` file while it is running.

`SERVE_WAIT` is one total startup budget (local default 1200 seconds; CI 3600).
`SERVE_STALL` remains 1200 seconds without log or target progress.
`SERVE_ATTEMPTS` defaults to 1; CI sets 2. Only inactivity can retry; a build error,
process exit or exhausted total deadline fails immediately. A retry keeps compiled
artifacts and downloaded tools. CI passes `--interactive false --open false
--verbose --locked` to dx.

Each attempt has `dx-PORT.attempt-N.log`; `dx-PORT.log` points to the current log.
Failures also write `dx-PORT.attempt-N.diagnostics.log` with owned executable names,
process state, disk availability and progress timestamps, never an environment or
argument dump. CI uploads logs and screenshots with seven-day retention, including
successful runs that recovered on the second attempt.

Run `python3 packages/web/tests/e2e/test-serve.py` for deterministic recovery and
ownership tests. They use fake time/HTTP and real isolated process trees, including
a stubborn descendant and an unrelated process that must survive cleanup.

`group-history` probes the committed CodeMirror bundle without a server: history checks never mutate, keyboard/menu group routing survives remounts, held input and stale cancellation tokens cannot cross a barrier, and undo/redo retain their opaque markers. Run through `run-all.sh group-history`.


`toolbar-icons` covers CodeMirror icons/settings and the Rust toolbar's complete
four-control budget, keyboard/touch overflow, Escape focus restoration, indentation
settings and compact phone height. Rust commands moved to ⋯ in WP-69; tests that
activate them by mouse must open `.mk-native-more > summary` first.


`native-editor-fps-input.py <desktop-fixture> --report <new-json-path>` runs
real compositor typing, Ctrl+Z/Ctrl+Shift+Z, selected replacement undo, held arrows
and small horizontal/vertical wheel input (including recorder interaction checks) in the FPS host. Source, documentation
preview and a 3 MB line are checked against exact canonical text. It owns and
stops a private Wayland display. This correctness test uses optional
`MOONKALE_FPS_TEXT_REPORT` and `MOONKALE_FPS_INPUT_REPORT` observers; leave both
unset for performance measurements. `MOONKALE_FPS_DOC_PREVIEW=1` selects the
preview mode at host creation. The passive input report retains bounded trusted
key/wheel/scroll observations and initial clock/geometry samples for diagnosing
hardware-specific input failures; it never edits the document.

Desktop recorder interaction checks use arrow keydown counts plus actual
caret/viewport movement, since WebKitGTK may omit `repeat`. Smooth small-scroll
checks observe actual ≤10 pixel document movements rather than assuming wheel
event units are pixels. Reports retain wheel axes/units and viewport travel;
wheel events without document movement cannot pass. The idle baseline now uses
the 95th percentile to resist short inter-refresh callbacks (P-225), with the
same ≤17 ms cap and unchanged strict/calibrated timing thresholds.

`graph-overview` and `graph-startup` serve the committed engine assets on a private
loopback ephemeral port (`MOONKALE_REPO` required); they need no app server.
The overview suite checks dense-graph panning, dot clicks and normal node drags.
Startup checks visible pixels at 100,000 nodes and reports JSON/stringify and
engine timings. `GRAPH_STARTUP_GATE=1` opts into a 3-second isolated engine gate;
use the bench lock and both heavy slots for timing. `MOONKALE_GRAPH_PAYLOAD`
optionally supplies the production panel JSON emitted by its ignored
`profile_store_payload` test. That file stays in the worktree's target directory.
These checks do not measure the desktop source/eval/compositor path.
`GRAPH_STARTUP_RUNS=6` reuses one engine/context for a warm-up and five samples;
report the last five. For a saved pre-fix engine, `MOONKALE_GRAPH_ASSETS` selects
its asset directory and `GRAPH_EXPECT_INITIAL_LAYOUT_ITERATIONS=1` explicitly
checks its original layout-before-draw behavior.

`responsive-tabs` checks selected tabs across two phone/wide round trips with
Graph foregrounded over the most recently opened document. `rich-images`
checks source-backed Rich images from a subfolder, escaped/remote path refusal
and canonical relative destinations after saving. `codemirror-search` checks
the German search panel, token-based controls and actual search selection.

`graph-camera` uses the isolated committed-engine harness to check frame-coalesced
camera mapping across resize/pan/zoom and embedded-node overlay suppression.
`canvas-layer` uses the production flow panel (`julia` feature): world-space card
following, click isolation, header drag with grouped undo/redo, zoom/off-screen
unmount and fit remount. Run it alongside flow, subflow, graph-flow, mtk and rxinfer.

`graph-pulse` uses pinned equal-length strokes and shared captures for both pulses. It rejects phase wraps and boundary peaks before comparing median forward velocities to the declared speeds; absolute displacement maxima are not a speed measurement (P-228). The existing low-detail animation suppression check remains.

`graph-egraph` runs on the isolated committed-engine harness, checking e-class
hulls, aggregate collapse/expand, rewrite layer visibility and automatic incoming
ancestry highlights. `graph-egraph-ui` uses the production Graph panel's paste
and translated controls against the normal app server. Both consume
`packages/graph-render/fixtures/egraph.json` through `MOONKALE_REPO`.

### Coarse graph tier (WP-06)

`graph-coarse.mjs` targets the separate hermetic fixture in
`packages/web/tests/fixtures/graph-coarse`, which embeds the **production** GraphPanel.
Start it with `CARGO_TARGET_DIR=<this-worktree>/target/agent dx serve --platform web
--port 8112 --interactive false --open false` from that fixture directory, through
the heavy slot helper. Stop that owned server when done. Run the suite with
`COARSE_PORT=8112` using the usual isolated Playwright directory. This suite is
not in run-all's default server list because it requires the separate fixture.
The fixture lockfile starts from the workspace's reviewed dependency versions;
no additional production dependency is introduced.

The lazy source has 1,000,501 nodes (500 folders × 2,000 files, folders and root),
but emits only the requested cuts. The suite gates first picture at two seconds,
checks the source count/tier, double-click expansion, stable camera, collapse and
Detailed/Auto overrides. A settled expanded pan must sustain a p95 frame interval
of at most 50 ms on software GL; this is a functional interactivity gate, not a
physical-display 60 FPS claim. The independent native test
`cargo test -p moonkale-core million_node_hierarchy -- --ignored --nocapture`
materialises a complete million-node hierarchy and tests the shared projection.

`graph-coarse-costs.mjs` uses the existing isolated committed-engine harness. It
records JSON size/encoding/first-draw at 100k coarse-sized nodes and times the
actual synchronous pointer hover handler, with a 16.7 ms p95 picking budget.
It does not include a native webview's Eval bridge. Run it with run-all or directly
with `MOONKALE_REPO` pointing at this worktree.

## Typed cells (WP-15)

`fixture.sh` seeds SQLite dates and typed.duckdb (timestamp, DOUBLE list and
FLOAT fixed array). table-grid.mjs checks their production source conversions.

Run table-typed.mjs separately against packages/web/tests/fixtures/typed-cells
on port 8112, using an owned Dioxus server and the Playwright working directory.
This production TablePanel fixture checks English/German/Chinese rerenders and
a Ref click opening its target in workspace state. It complements the SQL tests.


ADR-0030: `sophia-folder` uses the server's default Sophia folder cache.
Use `SERVE_FEATURES= .../serve.sh start` to check platform defaults without
explicit extra features. CI also runs it in its own job ("browser Sophia folder store", ADR-0029 gate 3).
It asserts the default graph draws cache names despite file truth, observes a
new definition through the watcher without Reload, and checks backend status
text while changing the persisted source setting.
Run with `graph watch search-trace` to cover the existing folder/index paths.

## Crash safety (WP-57)

`crash-safety.mjs` runs against a debug web build with
`SERVE_FEATURES=julia,rocksdb,helix,crash-test`. The `crash-test` feature adds a
synthetic extension only when debug assertions are enabled; release builds
contain no failure injection. It checks per-extension disablement, real wasm
panics in both an event and a task, the independent reload overlay, repeated
startup failures, and `?safe` without changing extension preferences. Native
`extension_crash` tests cover panic containment after an await and later event
callbacks; the wasm build currently aborts, so its real panics use the overlay.
Run `run-all.sh crash-safety shell menubar appearance extensions panels files rich stores palette wiki`.
The crash suite defaults to Firefox; `CRASH_BROWSER=chromium run-all.sh crash-safety`
checks the same real event/task panics and usable overlay in Chromium (P-234).
Native guard tests also use a 64 KiB future to check bounded wrapper size,
Pending/Ready polling, owner restoration, cancellation and drops.

The crash-test fixture additionally requires `?crash-test` (native: `MOONKALE_CRASH_TEST`) so enabling the test feature does not change ordinary startup panels.

`serve.sh` includes `crash-test` in its default test feature set. Explicitly
reduced feature builds should omit the crash-safety suite. The normal URLs used
by other suites do not construct the synthetic extension.


## Keyboard journey (WP-64)

`journey-keyboard.mjs` uses keyboard events only: Ctrl+O focuses the web folder
path, Enter opens it, quick open selects files, typing and Save persist text,
Tab/arrow keys switch document tabs, the palette opens Graph and closes editors.
It checks empty-tile hints; modifier delay, editor suppression, chord cancellation
and release cover key tips. The palette suite additionally checks Settings
rebinding and unbinding in palette rows and editor-button tooltips. `journey-keyboard-hold-ctrl.png` in `M1_SHOTS` records
the badges. Run it with `run-all.sh journey-keyboard`; Firefox and the normal
fixture suffice. The matching mouse journey and remaining discovery coverage are described below.

Graph renderer fixtures that replace the scene must first wait for the host's
initial node count to be positive and match the engine's `node_count()`.
A live GL backend alone means the canvas is ready, while the folder graph may
still be in flight and overwrite the fixture. `graph-edit`, `graph-layers`,
`graph-pulse` and `graph-flow` now use the same readiness check as the other
injected-scene suites (NEXT-28, 2026-10-28); their scene assertions are unchanged.

## Shortcut discovery and mouse journey (WP-64)

`journey-mouse.mjs` follows the keyboard journey using menus, Explorer rows,
editor buttons and document tabs. Its only keyboard operation types document
text; folder opening, saving, Graph and closing use the mouse. Run both with
`run-all.sh journey-keyboard journey-mouse`. The modifier screenshot remains
`journey-keyboard-hold-ctrl.png` in `M1_SHOTS`.

`shortcut-discovery.mjs` covers F1 and palette access, grouped/filterable effective
bindings, rebinding/unbinding, printable/slash search, input/editor protection,
persisted once-only mouse nudges and their disable setting, utility-panel focus, remote/server dialogs and the terminal chooser.
`graph-a11y` checks printable/slash search and focus restoration on the actual
Chromium-rendered canvas. `lsp2` checks CodeMirror rename/reference/action focus
and editor return; it also runs against the native editor with `LSP_EDITOR=native`. These three suites use the normal smoke feature set.

WP-61 input layout suites (smoke feature set): `mouse.mjs` exercises thumb-button
document/camera history, background tree opening, middle pan/tab closing and mouse
overrides. `keymaps.mjs` checks preset layering, F14/F24, German layout labels,
physical Slash on simulated QWERTZ/AZERTY/Dvorak and composition/AltGr exclusion,
plus focused left-hand graph controls. These use Chromium software WebGL.
The keyboard-layout cases inject DOM key/code pairs; they do not change the OS
keyboard or exercise a real IME. Desktop WebKit and Android input remain manual
platform checks.

`density.mjs` checks the Simple chrome budgets, Detailed, shared Settings/palette selection, extension chrome offers, legacy chrome migration, the Detailed Rust toolbar at phone width, and every registered command in both densities. `chrome-controls.mjs` reaches folded controls by clicking their real disclosure; it never changes density or enables chrome.

## Unsaved-buffer recovery (WP-56)

`recovery.mjs` is registered in run-all and CI smoke. It reloads with a dirty CodeMirror buffer, restores explicitly, saves and verifies cleanup, changes the file outside the app to exercise stale-base comparison, then discards and verifies a second reload is quiet. It uses a disposable fixture and reads browser state only to await durability.

## Named outer sessions (WP-46)

`sessions.mjs` is registered in run-all and CI smoke. It switches between two
sessions with different folders and dirty buffers, verifies each buffer on
return and unchanged files, then checks rename, JSON metadata download/import,
confirmed soft deletion and saving the surviving buffer. The old `session.mjs`
covers cross-window transport, rather than named workspace switching.

Named-session startup can immediately reopen folders and start their watcher
long polls. Reload checks therefore wait for DOM content and the relevant UI
state, rather than `networkidle`, which an open watched folder cannot guarantee.

`linked-panels.mjs` (Chromium/SwiftShader) checks two independent local graphs,
tab stripes and accessible group names, wiki navigation, group Back, lead close
rules, reload and named-session switching (WP-59). Included in the CI smoke build.
