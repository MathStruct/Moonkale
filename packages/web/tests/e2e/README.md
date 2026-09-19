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

```sh
SP=/tmp/moonkale-e2e; mkdir -p $SP/e2e && cd $SP/e2e && npm i playwright && npx playwright install firefox
packages/web/tests/e2e/fixture.sh $SP/m2root
packages/extensions/wordcount/build.sh   # with MOONKALE_CONFIG_DIR=$SP/cfg for the wasm-ext suite
(cd packages/web && MOONKALE_CONFIG_DIR=$SP/cfg MOONKALE_LLM=mock MOONKALE_ROOT=$SP/m2root dx serve --port 8090)
cp packages/web/tests/e2e/*.mjs $SP/e2e/   # playwright resolves from there
cd $SP/e2e && export PORT=8090 M1_ROOT=$SP/m2root M1_SHOTS=$SP
for s in menubar session graph links-sqlite terminal typst lsp ladybug agent search-trace settings rich agent-writes flow wasm-ext phone; do
  rm -rf $SP/m2root/.moonkale $SP/m2root/*.flow.json $SP/m2root/model.jl; printf '# Home\nSee [[Alpha]] and [[notes/Beta]] and [[Missing]].\n' > $SP/m2root/Home.md
  node $s.mjs
done
```

Milestone 6 suites: `flow.mjs` (enable Flow editor + Lux in Settings → Extensions, New Flow…, place/wire/reject/Generate/Save), `wasm-ext.mjs` (wordcount module listed, enabled with `read-sources`, its command runs as an agent tool after approval), `phone.mjs` (420 px viewport: one tile, bottom bar, Editor/Graph/Terminal/Agent/Settings switching, no layout persisted, widening restores the layout). `bench.mjs` prints layout ms/step for 1k–100k nodes (a measurement, not a test); `agent-live.mjs` needs a real key.
