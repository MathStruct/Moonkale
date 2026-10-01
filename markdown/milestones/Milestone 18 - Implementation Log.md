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

### Baseline (before any refactoring)
Measured on the dev machine (8 cores, 30 GB) unless stated.

| what | value |
|---|---|
| workspace | 38 crates, 43 600 lines of Rust; `ext-api/src/workspace.rs` 2 603 lines, 39 public signals, 94 public methods |
| tests | 132 Rust tests pass, 6 ignored; 47 browser suites |
| clean `cargo test --workspace --no-run` (no debug info, no incremental — as CI) | **290 s**, 9.6 GB of `target/` |
| then `cargo clippy --workspace --all-targets` | +42 s (11 GB) |
| then `cargo check -p web` for wasm32 | +26 s (11 GB) — so a cold `test` job is about 6 minutes of compiling here, more on a 4-core runner |
| release CI per job (run 36301090886, `260927-proto`, before Milestone 17) | Android 28 min · macOS arm64 42 min · Linux 71 min · macOS x86_64 89 min · Windows 98 min · Nix 114 min · Arch 116 min |
| release artefacts (`260927-proto`) | APK 23 MB · Arch 115 MB · deb 124 MB · tarball 124 MB · AppImage 142 MB · dmg 64/69 MB · msi 53 MB · NSIS 232 MB |

Not measured yet: cold open of this repository (needs the desktop app; `bench.mjs` measures the web build), and the release sizes *after* Milestone 17 (the next tag will show what the four stores added).
