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
