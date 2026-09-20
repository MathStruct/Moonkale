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

`graph.mjs`, `links-sqlite.mjs`, `terminal.mjs`, `typst.mjs`, `lsp.mjs`, `ladybug.mjs` expect a fixture folder as `MOONKALE_ROOT` containing: `Home.md` (`See [[Alpha]] and [[notes/Beta]] and [[Missing]].`), `Alpha.md`, `notes/Beta.md`, `data.sqlite` (any table), `report.typ`, a cargo crate (`Cargo.toml` + `src/main.rs` with `fn add(a: u32, b: u32) -> u32` and a deliberate `let wrong: String = total;`), and `people.lbug` (Person/City node tables, Knows/LivesIn rel tables — see `packages/sources-graph/tests/ladybug.rs` for the Cypher). `lsp.mjs` needs `rust-analyzer` on the server's PATH.

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

`history.mjs` (entity log: content/add/rename/remove/checkpoint events, active-file filter, text-at view, reload from `.moonkale/history.jsonl`), `presence.mjs` (two browser contexts with different names see each other on the status bar, tabs and Explorer; leaving clears), `graph3d.mjs` — **Chromium** with software WebGL (`npx playwright install chromium` once): renderer starts on WebGL2, labels drawn, the 3D toggle, orbit changes the frame. `wasm-ext.mjs` gained a step: the page is cross-origin isolated, the module ran in the browser (no `/api/ext/run` request) and keeps working with that endpoint blocked.

## Milestone 9

`duckdb.mjs` (the fixture's `data/people.csv` + `orders.csv`: open as views, join in the table editor, write refused). `history.mjs` gained restore-with-cause, `presence.mjs` the cursor gutter, `graph3d.mjs` a 3D drag. The desktop hub client has a native test instead: `MOONKALE_HUB=http://127.0.0.1:8090 cargo test -p desktop --test hub -- --ignored` against a running `serve.sh` server.

- `wiki.mjs` (spec 012): decorated links in source and rich mode, `[[` completion in both, click/Ctrl+click follows, create-on-click, Links panel *Create*, rename rewrites backlinks.
- `highlight.mjs` (spec 010): grammar tokens for Rust/Markdown/TOML, fold gutter, `Mod-/`.
- `panels.mjs` (spec 011): close/reopen static panels, side-bar collapse and return.
- `image.mjs` (spec 008): png/svg open in the viewer, zoom states, SVG Source.
- `shell.mjs` (spec 009): activity bar, badges, hide/show, Ctrl+B, registry-built menus, source row decoration.
- `android-cdp.mjs` / `android-pinch.mjs`: not suites — helpers for the phone over `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>` (evaluate JS; inject a pinch and a rotation, spec 006).
- `graph3d.mjs` gained a two-finger step (CDP `Input.dispatchTouchEvent`).
