---
title: "How to Write Tests"
tags: [testing, guide]
---
> [!note] Checked 2026-10-01
> The recipes for plain `#[test]`, integration tests in `tests/`, `VirtualDom` component tests and the Playwright suites match what the repository does. `proptest`, `insta`, `nextest`, `FakeSource` and `TestHost` are **recommendations, not in use** — no crate depends on them yet. Where the real tests are: [[Testing Strategy]].

Concrete recipes for each layer, in the order you'll meet them. Strategy and rationale: [[Testing Strategy]]. All snippets target Dioxus 0.7.10 (APIs checked: `VirtualDom::rebuild_in_place`, `dioxus_ssr::render`).

## 0. Where tests live and how to run them

```text
packages/core/src/graph/node.rs      # #[cfg(test)] mod tests { … }   ← unit tests next to the code
packages/core/tests/transactions.rs  # integration tests: use the crate as a consumer would
packages/shell/tests/workbench.rs       # component tests (render to string)
packages/server/tests/server_fns.rs     # server-function tests
packages/web/tests/e2e/…             # scripts driving a real browser
```

```sh
cargo test -p moonkale-core                 # one crate
cargo test --workspace --exclude web        # all native tests
cargo nextest run --workspace --exclude web # same, parallel processes (install: cargo install cargo-nextest)
cargo test -p moonkale-core -- ids::        # filter by name
cargo insta review                          # accept/reject snapshot changes
```

## 1. Unit tests for pure logic (`core`, `lift`, `extract`)

The bulk of the suite. No async, no I/O.

```rust
// packages/core/src/id.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_ids_are_deterministic() {
        let src = SourceId::new("folder-1");
        assert_eq!(NodeId::derive(&src, "src/main.rs"), NodeId::derive(&src, "src/main.rs"));
        assert_ne!(NodeId::derive(&src, "src/main.rs"), NodeId::derive(&src, "src/lib.rs"));
    }
}
```

Property tests for invariants (`proptest`):

```rust
proptest! {
    #[test]
    fn apply_then_undo_is_identity(ops in arb_transaction()) {
        let mut g = Graph::default();
        let applied = g.apply(&ops).unwrap();
        g.undo(&applied).unwrap();
        prop_assert_eq!(g, Graph::default());
    }
}
```

Snapshot tests for structures (`insta`) — e.g. the lifted schema graph of a SQLite file:

```rust
#[test]
fn sqlite_schema_lifts_to_graph() {
    let schema = introspect_sqlite("tests/fixtures/chinook.sqlite");
    insta::assert_yaml_snapshot!(lift::schema_graph(&schema));
}
```
First run writes `snapshots/…snap.new`; `cargo insta review` accepts it; later runs diff against it.

## 2. Component tests (render `rsx!` without a browser)

Dioxus components are functions; you can build a `VirtualDom`, run one render, and serialise it with `dioxus-ssr`. This tests structure, conditional rendering and props — not clicks.

```toml
# packages/shell/Cargo.toml
[dev-dependencies]
dioxus-ssr = "0.7"
insta = { version = "1", features = ["yaml"] }
```

```rust
// packages/shell/tests/workbench.rs
use dioxus::prelude::*;
use ui::EditorWorkbench;

#[test]
fn workbench_renders_all_panels() {
    let mut dom = VirtualDom::new(EditorWorkbench);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);

    for title in ["Explorer", "Search", "main.rs", "lib.rs", "Terminal", "Problems", "Outline"] {
        assert!(html.contains(title), "missing panel {title}");
    }
    assert!(html.contains("wb-status-bar"));
}
```

With props: `VirtualDom::new_with_props(SourceFile, SourceFileProps { name: "lib.rs".into() })`.

Testing **state changes** without a browser: drive the signal, not the DOM.

```rust
#[test]
fn search_shows_hint_when_empty() {
    let mut dom = VirtualDom::new(Search);
    dom.rebuild_in_place();
    assert!(dioxus_ssr::render(&dom).contains("Type to search"));
}
```
For "type into the input and see results", the logic (`query → results`) should be a plain function you unit-test; the component test only checks both branches render. If you find yourself needing to *click*, that's an E2E test.

Snapshot the HTML for shells and layouts (`insta::assert_snapshot!(html)`) — a changed snapshot is a cheap review of "did I mean to change the DOM?".

## 3. Server-function tests (`api`)

Server functions are ordinary `async fn`s on the server side. With the `server` feature on, call them directly:

```toml
# packages/server/Cargo.toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
// packages/server/tests/server_fns.rs
#[tokio::test]
async fn echo_round_trips() {
    let out = api::echo("hi".to_string()).await.unwrap();
    assert_eq!(out, "hi");
}
```
Run with `cargo test -p api --features server`. This tests the function body, auth checks you put in it, and its interaction with real sources — use a tempdir SQLite/DuckDB and a `FakeSource`.

For the **wire** (serialisation, routing, errors as seen by a client), start the server and use `reqwest`:

```rust
#[tokio::test]
async fn query_endpoint_rejects_unauthenticated() {
    let base = test_server::spawn().await;              // helper: binds api on a free port
    let res = reqwest::Client::new().post(format!("{base}/api/query")).json(&…).send().await.unwrap();
    assert_eq!(res.status(), 401);
}
```
Keep these few; they're slower and mostly guard auth/CORS/error mapping.

## 4. Integration tests for sources (real engines, temp files)

```rust
// packages/sources/sql/tests/sqlite.rs
#[tokio::test]
async fn foreign_keys_become_edges() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.sqlite");
    seed_sqlite(&db, "CREATE TABLE a(id INTEGER PRIMARY KEY); CREATE TABLE b(id INTEGER PRIMARY KEY, a_id REFERENCES a(id)); …");

    let src = SqliteSource::open(&db).await.unwrap();
    let view = src.query(Query::neighbours(row_id("b", 1), Direction::Out)).await.unwrap();

    assert_eq!(view.edges().filter(|e| e.kind == EdgeKind::ForeignKey).count(), 1);
}
```
- Embedded engines (SQLite, DuckDB, `lbug`) → tempdir, no infra.
- Networked engines (Postgres, Redis, Falkor, TypeDB) → `testcontainers` starts Docker containers per test module; mark `#[ignore]` so `cargo test` stays infra-free and run them with `--ignored` in CI.
- `lbug`/`duckdb` link C++: use `cargo nextest` so a crash in one test doesn't kill the whole binary.

## 5. Extension tests

`ext-api` will ship a `TestHost`:

```rust
#[test]
fn greet_increments_counter() {
    let host = TestHost::new().with_source(FakeSource::from_nodes([...]));
    let mut ext = Hello::default();
    ext.activate(host.handle()).unwrap();
    let out = ext.command("hello.greet", json!({"name": "x"})).unwrap();
    assert_eq!(out["message"], "Hello, x!");
    assert_eq!(host.recorded_calls().len(), 1);   // set_status
}
```
Same test runs for a static build and (via the wasm harness) for the component build — the point of one API.

## 6. Browser tests for wasm-only code

For code that only makes sense in a browser (the interop transport, OPFS backend):

```toml
[dev-dependencies]
wasm-bindgen-test = "0.3"
```
```rust
#![cfg(target_arch = "wasm32")]
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn eval_round_trip() { /* mount, apply, expect CustomEvent */ }
```
Run: `wasm-pack test --headless --firefox packages/editors/code` (Firefox is installed on the dev box). The JS bundles themselves get `vitest` unit tests inside `packages/js/<pkg>/`.

## 7. End-to-end

What we already did for the dummy UI, scripted:

```sh
(cd packages/web && dx serve --port 8080 &) ; until curl -sf localhost:8080 >/dev/null; do sleep 1; done
firefox --headless --profile /tmp/ff --window-size=1400,900 --screenshot /tmp/shot.png http://127.0.0.1:8080/
```
Assert on the DOM (dump `document.body.innerHTML` via a tiny Playwright script) or compare screenshots against a baseline with a tolerance. Start with 2–3 flows: app boots, a panel can be docked, a file opens. Add a WebKitGTK desktop E2E only when the graph surface work starts ([[ADR-0011 Desktop graph surface strategy]]).

### The desktop app on a private display

`tools/headless.sh` (2026-10-15) gives each worktree its own **headless Wayland display**: a sway with no screen. The desktop app runs there and is clicked, typed into and screenshotted, and nothing appears on the user's screen. Several agents can run one each at once.

```sh
tools/headless.sh start                       # sway + input; prints WAYLAND_DISPLAY
tools/headless.sh run MOONKALE_CONFIG_DIR=$PWD/target/headless/cfg \
    target/dx/moonkale/release/linux/app/moonkale
tools/headless.sh click 200 128               # left; or: click X Y right|middle
tools/headless.sh type "/path/to/folder"      # printable ASCII, US keymap
tools/headless.sh key Return ctrl+a ctrl+shift+p
tools/headless.sh hold right 5               # compositor key repeat; maximum 10 seconds
tools/headless.sh wheel vertical .25 70 .08   # pixel delta, count, gap in seconds
tools/headless.sh move 300 400                # absolute output coordinates
tools/headless.sh press                      # left button; move, then release to drag
tools/headless.sh release
tools/headless.sh shot target/headless/shot.png
tools/headless.sh stop                        # the apps it started, the input helper, sway
```

- **Tools:** `sway`, `grim`, `xkbcli` (libxkbcommon) and Python 3, all in `nix develop`.
- **Input** comes from `tools/headless-input.py`, which keeps one virtual pointer and one keyboard attached. It replaced `wlrctl` and `wtype`: each of their calls adds a device and removes it right away, and the app lost clicks and dropped letters at random.
- **Rendering:**
  - Apps get Mesa's software GL by default. With the NVIDIA driver, WebKit's web process crashes in `libnvidia-eglcore` on a headless sway once a pointer or a screen capture is involved, which is also why wayvnc was dropped.
  - `HEADLESS_GPU=1` opts out of software GL.
  - `HEADLESS_GL_THREADS=0..32` optionally controls Mesa’s `LP_NUM_THREADS` for profiling (0 disables render worker threads); unset keeps Mesa’s default. The desktop report records this override. See [Mesa environment variables](https://docs.mesa3d.org/envvars.html#envvar-LP_NUM_THREADS).
  - The graph canvas under software GL is not verified yet.
- **Typing speed** is about human speed (`HEADLESS_KEY_GAP`, 30 ms between key events). Faster typing loses keys in the app's controlled inputs.
- **State:** PIDs, logs, the input socket, sway's own runtime dir with its Wayland socket (`run/`) and the apps' private cache, data and config homes (`home/`, kept between runs so shader caches stay warm) are in `target/headless/` (or `$HEADLESS_DIR`), so nothing lands in `/run/user` and a sandbox that can write only the worktree can still run it. Logs: `moonkale.log`, `sway.log`.
- **Bounded gestures:** wheel accepts vertical/horizontal pixel deltas in [-1000, 1000], 1–200 events, gaps of 0.01–1 second and at most 15 seconds total. Hold accepts a named key or printable unshifted key, for at most 10 seconds; it releases the key in a `finally` block. Motion is bounded by the configured output (maximum 16384 pixels per axis).
- **Desktop frame harness:** `packages/web/tests/e2e/editor-fps-desktop.py --binary <release-fixture> --corpus <directory> --report <new-json-path>` drives all 21 production-panel cases on a private 1500×1100 display, verifies the selected case and observed interaction, writes raw/summary JSON and stops its display even on failure. It uses compositor input, including key repeat and small pixel wheel deltas; the recorder observes trusted WebView events. Its fixed coordinates are specific to this fixture. Software GL results measure CPU-side Rust/DOM/layout costs and do not establish hardware GPU compositing performance. A pass is necessary, not sufficient: Daniel’s real-desktop check is the final acceptance.
- **Timing precision:** the desktop recorder retains both callback-entry and rAF clocks. GTK WebKit quantizes both to 1 ms here; the report labels the calibrated vsync fallback separately from the original strict interval gate (ADR-0023). Acquire `~/.cache/moonkale-slots/bench.lock` and both heavy slots for measurements; the idle baseline must contain at least 30 frames and have a p95 no slower than 17 ms. Software GL also includes CPU painting/compositing costs, not just editor layout.
- **Still for Daniel:** real keyboard, IME and pointer behaviour on his own KDE session.

## 8. How I'd go about it, day to day

1. **Write the fake first.** Before the first real `Source`, write `FakeSource` (in-memory nodes/edges, records calls). Every editor and the shell test against it; real sources get integration tests that prove they behave like the fake.
2. **One test per rule in the vault.** Lifting rules in [[Data Sources]], id determinism in [[Graph-Native Model]], permission checks in [[Host API Reference]] — each bullet is a test name.
3. **Snapshot the shell, assert the logic.** Component snapshots catch accidental DOM changes; put decisions in plain functions and unit-test those.
4. **Reproduce bugs as tests.** Every `P-nnn` in [[Problem Log]] that has a code fix gets a regression test named after it.
5. **Keep the fast suite under a minute.** Move anything slower to `#[ignore]` + CI.
6. **Platform gates are tests too.** `cargo check -p web --target wasm32-unknown-unknown` is the test that says "no native crate leaked"; wire it into `cargo xtask check` so it isn't forgotten.

Desktop FPS preflight samples aggregate `/proc/stat` activity before each mode and gesture. It requires two consecutive samples at or below 10% active CPU within six attempts; all samples are retained in the summary. Settling occurs before input, never by discarding a failed gesture. Hold the bench lock and both heavy slots throughout the run.


## 8. Parser fuzzing and dependency gates (WP-47)

`fuzz/` is a standalone cargo-fuzz crate with its own workspace and lockfile. It
includes the actual production source files instead of copying parser code or
compiling the Dioxus UI. Ordinary `cargo test --workspace` needs no nightly or
libFuzzer build. Use a **separate target directory for this standalone workspace**
(P-241): its graph-render cdylib/rlib artifacts can otherwise collide with the
application workspace's serde feature context. Both directories stay inside your
own worktree. Check its existing parser regressions on stable:

```sh
CARGO_TARGET_DIR="$PWD/target/agent-fuzz" cargo test --manifest-path fuzz/Cargo.toml --locked --lib
cargo fmt --manifest-path fuzz/Cargo.toml --check
```

Install cargo-fuzz **0.13.2** with `cargo install cargo-fuzz --version 0.13.2 --locked`.
Use a nightly with its rust-src component if building the standard library (CI
pins nightly-2026-09-29). Run heavy commands through your slot wrapper and keep
`CARGO_TARGET_DIR` absolute and inside your own worktree:

```sh
CARGO_TARGET_DIR="$PWD/target/agent-fuzz" cargo +nightly fuzz run math_delimiters -- -max_total_time=60 -max_len=65536 -rss_limit_mb=2048 -timeout=10
CARGO_TARGET_DIR="$PWD/target/agent-fuzz" cargo +nightly fuzz run flow_format -- -max_total_time=60 -max_len=65536 -rss_limit_mb=2048 -timeout=10
```

`math_delimiters` exercises the existing Markdown display-math range parser,
checking ordered UTF-8 ranges and the 16 KiB TeX bound. WP-36's planned delimiter
normaliser is not built on this branch and must get its own target when added.
`flow_format` exercises `Flow::parse` and successful JSON serialization/reparsing.
The harness limits are smoke-test budgets, not proof that the production parsers
have sufficient bounds. Seed corpora include Unicode and code/metadata exclusions
and an empty/subflow document. Normal parser tests compile in the standalone crate
too. The new libFuzzer harness dependency is isolated to that crate; its exact
version matches the existing production lockfile's transitive libfuzzer-sys.

CI runs separate non-blocking jobs. Crashes land in `fuzz/artifacts/<target>/`
and are uploaded even after failure. Reproduce with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<crash-file>`. Reduce with
`cargo +nightly fuzz tmin <target> <crash-file>`. Before a fix, add the minimized
input and assertion as a normal `#[test]` in
`packages/code-view/src/native_math_parser.rs` or `packages/ext-api/src/flow.rs`;
verify it fails before and passes after the fix. Do not commit generated corpora,
coverage, targets or crash dumps containing private source. Commit only reviewed
seeds/regression inputs. The initial local 60-second runs found no crash. CI subsequently found P-212:
`p-212-crash.md` is the original 128-byte input; the other three P-212 seeds are
nine-byte minimized/substituted inputs. They also feed stable production tests
(`cargo test -p moonkale-code-view --lib` and
`cargo test -p moonkale-editor-agent --lib`). The production guard bypasses NUL,
vertical tab and form feed before parsing on all profiles; unwind builds also
catch failed complete parse operations. Do not remove the control guard after a
native-only catch test passes: the wasm release uses `panic=abort`.

Dependency checks: `python3 tools/check-security.py rust` requires cargo-deny
0.20.2 and cargo-audit 0.22.2. `--db-root <writable-directory>` relocates the two
RustSec caches without changing `deny.toml`. `python3 tools/check-security.py js`
audits all checked-in JS lockfiles without installs or scripts (npm registry
access required). `python3 tools/check-vendored.py` and
`python3 tools/test-security.py` run offline. New findings fail; existing dated
exceptions and their limitations are in [[Security]]. To update vendored bytes,
verify upstream/version and documented local patches first, then change the
reviewed SHA-256 entries in `tools/vendored-assets.json` explicitly; CI never
fetches or silently regenerates them.

## Platform CI (WP-26, 2026-10-17)

`.github/workflows/ci.yml` keeps the full workspace, clippy, wasm check and
browser suites on Ubuntu. The `portable` matrix adds `windows-latest` and
`macos-14` (Apple silicon), initially **advisory**, with a 45-minute timeout
per OS and separate Rust caches. It runs core, project-fs, index, ext-api,
LLM and Unison tests, plus remote library tests, then checks the default
extension distribution. These defaults match the desktop release's extension
selection (no Julia); LadybugDB is enabled only by desktop's Linux target table
(P-144). This checks the distribution, not the desktop binary, installers or UI.

Portable filesystem tests retain forward-slash native keys (the source API's
format), canonicalize paths used by content-open instrumentation, use temporary
folders and write fixture bytes explicitly, so checkout
CRLF conversion cannot alter them. General rename tests use distinct basenames
for case-insensitive volumes. Plain relative-path jail validation runs everywhere;
Unix symlink fixtures require no Windows privilege setup and remain Unix-only.
The FIFO fixture uses the Linux/Android-only rustix dependency. Watcher tests
exercise native recursive watching on Windows/macOS. Remote library tests cover
its platform branches without the Unix shell/server integration shim; the
PTY handshake fixture uses Unix `sleep` and stays Unix-only. Unison's
existing Unix symlink fixture remains gated; its other unit tests run on both. The UCM integration fixture skips when UCM is
absent; these jobs do not install it.

The first GitHub runs still need review: Linux validation cannot establish native
Windows/macOS success. Once both OS jobs are green, remove `continue-on-error:
true` from **portable** and remove `(advisory)` from its name; branch protection
must also require both matrix checks if desired. Intel macOS remains release-only.

WP-07 adds the standalone `egraph` fuzz target and its reviewed saturation seed.
Run `cargo +nightly fuzz run egraph -- -max_total_time=60 -max_len=65536
-rss_limit_mb=2048 -timeout=10` through the heavy-slot wrapper from the repository
root, with a worktree-local `CARGO_TARGET_DIR`. It exercises the production
bounded parser, incoming rewrite walk and aggregate collapse/expand delta path.
It is not yet part of the two-target nightly matrix.


### Session exchange parser (WP-46)

`cargo +nightly fuzz run session_bundle -- -max_total_time=15 -max_len=65536 -rss_limit_mb=2048 -timeout=10`
exercises the production version/byte/nesting boundary in `moonkale-state`.
Use a worktree-local target and the heavy slot helper. The parser caps actual
imports at 4 MiB and retains serde's recursion limit; unit tests cover oversized
and deeply nested inputs. The versioned seed is intentionally tracked. The
2026-10-28 run completed 3,712,726 inputs without a crash. This is parser coverage,
not a claim that arbitrary document or conversation text can be exported safely;
session exchange carries metadata and secret references only.
