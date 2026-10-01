---
title: "Milestone 18 — Implementation Log"
description: What was done for the library refactor, phase by phase — starting with phase 0, the safety net (CI, layering rules, baseline numbers, app state out of git).
tags: [milestone, log, refactor]
---
Plan: [[Milestone 18 - Library Refactor]]. One section per phase; renames get a table (old path → new path → why) in the phase that does them.

## Phase 0 — Safety net (2026-10-01)

| # | step | outcome | notes |
|---|---|---|---|
| 1 | Local state before CI: `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` | fmt: 2 files off (`sources-sql/src/text.rs`, `web/src/main.rs`, both from the merged audit PRs); clippy: **1 warning** (`needless_borrow`, `text.rs:27`); tests: **132 passed, 0 failed, 6 ignored** | fixed both; `clippy -D warnings` is clean, so CI can be strict from the start. The 6 ignored tests need a live server, hub or sshd (`desktop/tests/{hub,remote}.rs`, `remote/tests/shim.rs`) |
| 2 | `.github/workflows/ci.yml` | three jobs on every branch push and PR: **lint** (rustfmt, layering rules), **test** (`cargo test`, `clippy -D warnings`, `cargo check -p web --target wasm32-unknown-unknown --features web`), **e2e** (fixture, `dx serve`, six Firefox suites: `files rich stores palette shell wiki`) | rust-analyzer is installed as a component because `lsp-local/tests/rust_analyzer.rs` talks to a real one; clang/libclang for RocksDB's bindgen and arborium's C; disk is freed first; `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` |
| 3 | `tools/check-deps.py` + `tools/deps-allow.txt` | the rules of the plan's "Target shape" as a script over `cargo metadata`: **29 known violations** listed, grouped by the phase that removes them; the check fails on a new one **and** on a listed one that is fixed, so the list only shrinks | rules: `core` depends on no workspace crate; `ext-api` only on `core`/`lsp`/`terminal`/`ext-abi`; only apps (and later `distribution`) depend on drivers or extensions; the shell's dependency tree holds none of duckdb, lbug, rocksdb, turso, helix-db, redb, rusqlite, wasmtime, reqwest. Evaluated on the default feature set |
| 4 | `.moonkale/` out of git | `history.jsonl` and `settings.json` untracked (`git rm --cached`, the files stay on disk); `.gitignore` ignores `.moonkale/*` except `katex.json` and `extensions/` | **on another clone**: pulling this commit deletes those two files there (or stops with "would be overwritten" if they changed) — copy them aside first if that machine's layout or history matters |
| 5a | The smoke batch locally (`run-all.sh files rich stores palette shell wiki`, Firefox, a warm `dx serve`) | ✅ all six pass in 45 s | the CI workflow itself is unverified until the branch is pushed |
| 5 | `serve.sh`: `SERVE_WAIT` | the first-build wait is configurable (default 240 × 5 s); CI sets 600 | a cold debug build of server + client on a 4-core runner can exceed 20 min |
| 6 | Baseline numbers | below | |
| 7 | First CI run (36914570665) | lint ✅ (38 s) · e2e ✅ (28 min, cold) · **test ❌** after 54 min: `lbug`'s build script fetched the *latest* LadybugDB release (v0.21.2, published that day) and got a 404; its source fallback is broken too | **P-146**: `LBUG_VERSION = "0.21.0"` pinned in `.cargo/config.toml` |
| 8 | Second run (36924574219) | lint ✅ · e2e ✅ (11 min, warm cache) · tests ✅ **132 passed** · **clippy ❌**: the runner's `stable` was Rust **1.99** (released 2026-09-28), which adds `needless_borrows_for_generic_args` (`ui/src/history.rs:190`); this machine had 1.98.1 | fixed; checked with `cargo +1.99.0 clippy` (installed next to the default toolchain, which is unchanged). CI stays on `stable` on purpose: a new release's lints show up in CI first — fix them then, and run `cargo +<new> clippy` locally to catch them all in one round |
| 9 | Third run (36927844507) | ✅ **all green**: lint 35 s · e2e 5 min · test + clippy + wasm 37 min | phase 0 done. The test job is the slow one (a cold-ish cache for the 1.99 toolchain; later runs should be faster) |

### Baseline (before any refactoring)
Measured on the dev machine (8 cores, 30 GB) unless stated.

| what | value |
|---|---|
| workspace | 37 crates, 43 600 lines of Rust; `ext-api/src/workspace.rs` 2 603 lines, 39 public signals, 94 public methods |
| tests | 132 Rust tests pass, 6 ignored; 47 browser suites |
| clean `cargo test --workspace --no-run` (no debug info, no incremental — as CI) | **290 s**, 9.6 GB of `target/` |
| then `cargo clippy --workspace --all-targets` | +42 s (11 GB) |
| then `cargo check -p web` for wasm32 | +26 s (11 GB) — so a cold `test` job is about 6 minutes of compiling here, more on a 4-core runner |
| release CI per job (run 36301090886, `260927-proto`, before Milestone 17) | Android 28 min · macOS arm64 42 min · Linux 71 min · macOS x86_64 89 min · Windows 98 min · Nix 114 min · Arch 116 min |
| release artefacts (`260927-proto`) | APK 23 MB · Arch 115 MB · deb 124 MB · tarball 124 MB · AppImage 142 MB · dmg 64/69 MB · msi 53 MB · NSIS 232 MB |

Not measured yet: cold open of this repository (needs the desktop app; `bench.mjs` measures the web build), and the release sizes *after* Milestone 17 (the next tag will show what the four stores added).

## Phase 1 — Prune (2026-10-02)

| # | step | outcome | notes |
|---|---|---|---|
| 1 | Design stubs out of the code | **24 files removed** — the 21 listed in the plan plus three orphans no module declared (`ext-api/src/{host,capability}.rs`, `llm/src/embed.rs`; `editors/code/src/{decorations,document}.rs` and `backend/native.rs` were orphans too) — and the crate **`editors/graph-desktop`** (36 workspace crates now). Their text moved first: driver designs → [[Data Sources]] ("Driver designs not built yet"), the native overlay → [[ADR-0011 Desktop graph surface strategy]] ("Plan B, as it was sketched in code"), decorations/document/native backend → [[Code Editor]], capabilities → [[Host API Reference]] | the one doc example in `core/command` was the 6th "ignored" test; 5 remain |
| 2 | Empty features | `typedb`, `falkor` (sources-graph), `redis` (sources-kv), `postgres` (sources-sql) removed; nothing enabled them | |
| 3 | P-135, the profile name | the renderer's profile is `[profile.graph-wasm]` (`build.sh` updated; output byte-identical, 3 870 601 bytes). The **web client** then built with dx's defaults: **54.9 MB** shipped wasm (62.1 MB raw), against **46.2 MB** in `260927-proto` (that release predates Milestone 17, so not every byte is the profile). Decided to keep the client size-optimised, but deliberately: its own `[profile.wasm-release]` with a comment saying why | with the explicit profile: **46.2 MB** again (46 197 411 bytes vs 46 164 510 in the release), so the whole 8.7 MB difference was the profile, not Milestone 17 |
| 4 | Release name in the binaries | `moonkale_core::VERSION` = `MOONKALE_RELEASE` at build time if set and non-empty, else the crate version; used by `moonkale-server --version`, MCP's `serverInfo` and `remote::session::VERSION` (the directory the server is uploaded to on SSH hosts, so a host now gets the new server with each release). `release.yml` sets `MOONKALE_RELEASE` from the tag for every job and passes it through the Arch container's `sudo` | verified: `0.1.0` without, `261002-proto` with, `0.1.0` when empty; changing it rebuilds `core`. The Nix build cannot see it (sandbox) and reports `0.1.0` |
| 5 | Template READMEs, stale crate docs | `api`, `desktop`, `ui` READMEs point at their crate notes; `web` got a real README (it had no crate note); crate docs of `api` (said "no authentication"), `ext-api` ("Milestone 1 scope"), `core`, `sources`, `sources-*` rewritten to what is built; crate notes `core.md`, `sources.md`, `sources-graph.md`, `sources-kv.md` updated | |
| 6 | **Issue #16**, Julia/Python symbols | `extract::LANGUAGES` + `extract::reads()` — one list for what the walk fetches and what `extract_into` dispatches; new test `julia_and_python_symbols_are_indexed` (fails without the fix: no symbols at all) | the same list also brings `.jl`/`.py` text into search |
| 7 | Checks | fmt ✅ · layering 29 known, 0 new ✅ · clippy `-D warnings` on 1.98.1 **and** 1.99.0 ✅ · **133 tests pass**, 5 ignored ✅ · wasm32 check ✅ | CI: see below |
| 8 | CI (run 36934297024) | ✅ all green: lint 34 s · e2e 9 min · test + clippy + wasm 12 min (warm cache) | phase 1 done |
