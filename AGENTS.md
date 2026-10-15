# AGENTS.md — start here

You are working on **Moonkale**, a graph-native code and knowledge editor in Rust and Dioxus 0.7. It opens folders and database connections, shows everything as one graph, and is driven by extensions. It runs on the desktop (Linux, plus macOS and Windows builds), the web (wasm client + server) and Android. Its long-term target is the [North Star](markdown/architecture/North%20Star.md): code as a graph in a database, Unison-like, where several languages, IRs, e-graphs and type systems live on one graph, proofs are annotations, and the code graph meets a knowledge graph. The near-term driver is the factor-graph editor for Lenticulum.jl ([Julia and Lenticulum](markdown/architecture/Julia%20and%20Lenticulum.md)).

`markdown/` is the design record (an Obsidian vault, published as the website). Read it before changing things; update it when you do.

## Read first

1. `markdown/Home.md`: the map of the vault.
2. `markdown/Status.md`: what is built. Check its date; where the code disagrees, the code wins, so fix the page.
3. `markdown/problems/Work Packages.md`: **pick your work here.** With several agents at once, read `markdown/problems/Running Several Agents.md` too (slots, ports, who merges, who writes memory). It has the open work as packages, the hot-spot owners, the reserved ADR and P- numbers, and the definition of done.
4. `markdown/architecture/Project Structure.md` and `markdown/Development.md`: crates, building, running.
5. `markdown/testing/How to Write Tests.md`, `packages/web/tests/e2e/README.md`: unit tests and the browser suites.
6. `markdown/specifications/README.md`: Daniel's small numbered specs, and the planned, unnumbered ones.
7. The ADRs in `markdown/decisions/`: the decisions that hold. Don't relitigate an accepted one without a new ADR.

## How the agents work together

- **Worktrees.** Claude keeps `master` and the main checkout (`~/Code/Moonkale/Moonkale`). Every other agent works in its own worktree (`~/Code/Moonkale/Moonkale-<agent>`) on its own branch: `codex`, `dev-mistral`, `dev-claude`, or `wp-NN-<slug>` for a work package. Contributor branches merge into master only when Daniel says so.
- **Target dirs.** Each worktree builds into its own `CARGO_TARGET_DIR` (for example `target/agent`). Never share another worktree's target dir. Never build in `/tmp`: it is tmpfs and too small.
- **Handoffs.** Notes between agents are untracked `*-from-<agent>.md` files in the receiver's worktree root (`HANDOFF-…`, `NEXT-…`, `REVIEW-…`). Read yours when you start.
- **Numbers.** ADR and P- numbers come from the reservation table in Work Packages. Problems go into `markdown/problems/Problem Log.md` with `markdown/templates/Problem Template.md`; decisions use `markdown/templates/ADR Template.md`.

## Before you commit: the gates (what CI runs)

```sh
cargo fmt --all --check
python3 tools/check-deps.py        # layering rules
python3 tools/check-colors.py      # no hard-coded colours outside the theme
python3 tools/check-strings.py     # every UI string in en, de and zh-CN, every id used
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p moonkale-distribution --no-default-features
cargo check -p moonkale-distribution --features julia
RUSTDOCFLAGS="-D warnings" cargo doc -p moonkale-core -p moonkale-ext-api -p moonkale-graph-render --no-deps
cargo check -p web --target wasm32-unknown-unknown --features web
```

Then the browser suites of what you touched:

```sh
MOONKALE_E2E=~/.cache/moonkale-e2e-<agent> PORT=<your port> packages/web/tests/e2e/serve.sh start
MOONKALE_E2E=… PORT=… packages/web/tests/e2e/run-all.sh <suite> …
packages/web/tests/e2e/serve.sh stop
```

## Rules that have bitten before

- **Never `pkill dx`** (or any `pkill -f` pattern that matches other sessions): it kills Daniel's desktop dev server. Stop your server with `serve.sh stop`; it tracks the PID.
- **Never commit `.secrets/`.** Never use a bare `git stash`: the stack is shared across worktrees. Use a WIP commit, or a stash with a unique message applied by SHA.
- **Before a fast-forward or merge, check the incoming paths against ignored files in the way.** A checkout once replaced the ignored `.secrets/` with a tracked link (P-163).
- **Pushing tags or contributor branches** to `MathStruct/Moonkale` needs Daniel's go-ahead. Push over https (`https://github.com/MathStruct/Moonkale.git`); there is no ssh agent.
- **The graph engine's wasm is committed** (`packages/graph-assets/assets/graph_render*`). After changing `packages/graph-render`, rebuild it with `bash packages/graph-render/build.sh` and commit the output.
- **The library crates are versioned by tag** (`ext-api-YYYY-MM-DD`, see `packages/ext-api/CHANGELOG.md`). A change that can break a caller gets a CHANGELOG entry, and the next tag carries it. `MathStruct/moonkale-julia` (Lux, ModelingToolkit and RxInfer flow libraries) depends on such a tag. Moonkale pulls it behind the `julia` feature, by its own tag. While developing both, use uncommitted `[patch]` sections.
- **UI strings** live in each crate's `locales/{en,de,zh-CN}.ftl`. Daniel reads German and some Chinese.
- **Security rules for every change** are at the end of `markdown/architecture/Security.md`: the server decides, no secret values anywhere, content is data, bounded parsers, safe connections, audited dependencies, and the page kept current. Apply them in review.
- **Keep docs in step:** the feature's page, `Status.md`, and `markdown/extensions/Extension Catalogue.md` whenever an extension is added, removed or moved.

## Dioxus 0.7 in brief

This repo uses Dioxus **0.7**; older APIs (`cx`, `Scope`, `use_state`) are gone.
- Components are `#[component] fn Name(props…) -> Element`. Props are owned (`String`, `Vec<T>`), `Clone + PartialEq`. Wrap heavy props in a pointer-compared newtype rather than cloning them each render.
- State: `use_signal`, `use_memo`, `use_resource`, plus context (`use_context_provider` / `use_context`). Read with `.read()` or `sig()`, write with `.write()`, `.set()` or `.with_mut()`; read without subscribing with `.peek()`.
- Don't hold a signal guard across an `.await`; `clippy.toml` bans it. Don't read and write the same signal in one expression (take the value first).
- `rsx!` takes loops and `if` directly; iterators go in braces. Use `key:` on list items.
- Element events used here: `onmounted` (gives `MountedData`: `get_client_rect`, `scroll`, `set_focus`), `onresize`, `onscroll` (`scroll_top`, `client_height`, …), keyboard and pointer events.
- `document::eval` is the escape hatch to JS. Avoid it for editor state ([JS Interop Boundary](markdown/architecture/JS%20Interop%20Boundary.md)).
- Reference: https://dioxuslabs.com/learn/0.7
