---
title: "P-243 — Plot step 2 review findings"
tags: [problem, security, plot, sql]
status: resolved
phase: WP-40
---
**Status:** resolved before merge · **Related:** [[Plotting Requirements]], [[Security]]

## Problem

agy's WP-40 step 2 (source aggregation, `.plot.json` specs, SVG export) stopped
at its quota while running tests (2026-11-08). Before merging it, Codex reviewed
the diff (the second reviewer for query text and paths), and Mistral's review
left probes without a report (turn limit), which Claude ran. Each finding was
first reproduced by a failing test; the tests stay as regression tests.

## Triage (2026-11-18)

| # | Finding (reviewer) | Right? | Done |
|---|---|---|---|
| 1 | High: a spec's `source.path` naming a link inside the folder opened an outside database (`open_folder` without a containment check) (Codex) | yes, reproduced by Codex with the real openers | `spec_source_inside` asks the folder's jailed `path` lookup first (not `node_at_path`, which falls back to a tree walk that lists outside links); test `plot/tests/spec_source.rs` |
| 2 | A source column named `b` captured `GROUP BY b`: SQLite/Turso put every row in bin 0, DuckDB refused (Codex) | yes, failed on all three engines | the bin is computed in an inner query exposing only `mk_bin`; `collision` case in `table_query.rs` |
| 3 | Composite primary keys chose the wrong 100 bar categories (`MIN` of the first key column only) (Codex) | yes, failed on all three | first appearance is `ROW_NUMBER() OVER (ORDER BY` the whole key `)`; `cats2` case |
| 4 | A finite range whose `hi - lo` overflows (±1e308) reported every row omitted (Codex) | yes, failed on all three | split width `hi/40 - lo/40`, bins as `x/w - lo/w`, midpoints by interpolation; `huge` case. The plot still reports such rows omitted, as the client path always did: an axis cannot span that range |
| 5 | Log-bin edges compared in raw space vs. floored `log10` in the reference may differ at exact boundaries (Codex, unverified) | not reproduced through the Rust code | no change |
| 6 | A bar whose SUM overflows f64 failed the whole aggregate ("lossless row versions … unavailable") (Mistral's probe `catsinf`) | yes, failed on all three | the overflowing sum becomes NULL and its category counts as omitted; `catsinf` case |
| 7 | `catsrev` kept-categories differ from the naive reference (Mistral's probe) | no: the reference had no 100-category cap; the first 100 in row order is the policy | none |
| 9 | Re-review, high: the guard checked the jail-normalized path (whitespace, `./` trimmed) but the panel opened the raw one, so a link `" inside.sqlite"` → outside still opened (Codex) | yes, reproduced by Codex with the real parser, guard and opener | `spec_source_inside` returns the normalized path it checked and the panel opens exactly that; whitespace case in `spec_source.rs` |
| 10 | Re-review: `spec_source.rs` imports `std::os::unix` unconditionally, so it fails to compile on Windows (Codex) | yes | `#![cfg(unix)]` |
| 11 | Re-review, low: for ranges wider than f64 can subtract, a value just below a bin edge can land one bin up (`x/w - lo/w` rounds) (Codex) | yes, but rows are kept and only those extreme ranges are affected | not fixed; recorded here |
| 8 | `plot.mjs` saved-spec state captured before the source aggregate replaced the held rows (found running the suite) | yes, test timing | the test reads the state in the same call that sees the aggregate (`aggState`), asserts not partial |
| 12 | Observed while fixing 8: right after Save plot, the spec's tab can briefly fall back to the held rows (marked partial) while it re-requests the aggregate | yes, a transient flicker; the final state is right and survives reload | not fixed; candidate for the PLT-09 work, which reworks data updates |

## Lesson

The jailed lookup and `Workspace::node_at_path` are not the same check: the
latter's fallback walks the tree, which lists links leading outside. A
containment check on content-provided paths must use the source's jailed query,
and must hand back the exact path it checked for the caller to open: two
normalizations of one string are two different paths.
