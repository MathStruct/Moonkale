---
title: "P-244 — Live plot review findings"
tags: [problem, plot, table, sources]
status: resolved
phase: WP-40
---
**Status:** resolved before merge · **Related:** [[Plotting Requirements]], [[Table Editor]]

## Problem

Claude's WP-40 live plots (PLT-09, 2026-11-18) went to two reviewers before
commit. Mistral (`review-rust`, headless) stopped at its turn limit after most
of its commands were refused, with one finding; Codex (read-only `exec` with
the requirement) reported two.

## Triage (2026-11-18)

| # | Finding (reviewer) | Right? | Done |
|---|---|---|---|
| 1 | `tests/changes.rs` covers SQLite and DuckDB data folders only, not Turso or a DuckDB file (Mistral) | yes | two tests added: a Turso file and a DuckDB file written by another connection are reported |
| 2 | A change during a pushed-down grid's first read is lost: the grid started from the epoch at mount, and the panel consumed the epoch without re-running, so grid and plot stayed stale until the next write (Codex) | yes, by trace (`panel.rs`: `seen_epoch` set at mount; parent effect skips `run` when the grid reloads). Not reproduced by a test: it needs a write between the read's snapshot and its answer | the query records the epoch when it is sent (`loaded_epoch`); the grid starts from it, so a newer epoch reloads at once; non-pushed tables re-run for the same reason |
| 3 | `data_epochs` was bumped for every source family, so a table over a store re-ran (and lost its grid state) on the store's changes, which before followed only the index/document path (Codex) | yes | only SQL sources get a data epoch; regression assertion in `ext-api/tests/follow.rs` (a folder's changes leave `data_epochs` empty) |
