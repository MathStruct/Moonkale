---
tags: [testing, guide]
---
# How to Write Tests

Concrete recipes for each layer, in the order you'll meet them. Strategy and rationale: [[Testing Strategy]]. All snippets target Dioxus 0.7.10 (APIs checked: `VirtualDom::rebuild_in_place`, `dioxus_ssr::render`).

## 0. Where tests live and how to run them

```text
packages/core/src/graph/node.rs      # #[cfg(test)] mod tests { … }   ← unit tests next to the code
packages/core/tests/transactions.rs  # integration tests: use the crate as a consumer would
packages/ui/tests/workbench.rs       # component tests (render to string)
packages/api/tests/server_fns.rs     # server-function tests
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
# packages/ui/Cargo.toml
[dev-dependencies]
dioxus-ssr = "0.7"
insta = { version = "1", features = ["yaml"] }
```

```rust
// packages/ui/tests/workbench.rs
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
# packages/api/Cargo.toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
// packages/api/tests/server_fns.rs
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
// packages/sources-sql/tests/sqlite.rs
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

## 8. How I'd go about it, day to day

1. **Write the fake first.** Before the first real `Source`, write `FakeSource` (in-memory nodes/edges, records calls). Every editor and the shell test against it; real sources get integration tests that prove they behave like the fake.
2. **One test per rule in the vault.** Lifting rules in [[Data Sources]], id determinism in [[Graph-Native Model]], permission checks in [[Host API Reference]] — each bullet is a test name.
3. **Snapshot the shell, assert the logic.** Component snapshots catch accidental DOM changes; put decisions in plain functions and unit-test those.
4. **Reproduce bugs as tests.** Every `P-nnn` in [[Problem Log]] that has a code fix gets a regression test named after it.
5. **Keep the fast suite under a minute.** Move anything slower to `#[ignore]` + CI.
6. **Platform gates are tests too.** `cargo check -p web --target wasm32-unknown-unknown` is the test that says "no native crate leaked"; wire it into `cargo xtask check` so it isn't forgotten.
