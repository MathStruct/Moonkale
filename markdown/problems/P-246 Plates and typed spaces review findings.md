---
title: "P-246 — Plates and typed spaces review findings"
tags: [problem, flow, rxinfer, units, review]
status: resolved
phase: WP-37
---
**Status:** resolved before merge · **Related:** [[ADR-0032 Plates and typed spaces]], [[Flow Editor]], [[P-245 Factor subsystem review findings]]

## Problem

Claude's WP-37 (plates and typed spaces, 2026-11-18/19) went to two reviewers before commit, given the diff and ADR-0032 only: Codex (read-only, static) and Mistral (`review-rust` in its worktree; its `cargo test` probes ran this time). Each finding taken was first shown by a failing test; the tests stay.

## Triage (2026-11-19)

| # | Finding (reviewer) | Right? | Done |
|---|---|---|---|
| C1 / M-F1 | `Unit::parse("m^-2147483648")` and `unit_term("'u^-2147483648")` panic (`i32::MIN.abs()`) in debug, wrap in release (Codex, Mistral independently) | yes, reproduced | the power is checked with `(-12..=12).contains`; test `hostile_units_are_refused_not_wrapped` |
| C2 | A scale beyond `f64` (`Gm^12*Gm^12*Gm^12`) becomes `∞`, and `∞ = ∞` makes different scales equal | yes, reproduced | `mul`/`pow` refuse a non-finite scale or one outside 1e±200; same test |
| C3 | Saturating exponents equate different dimensions (`m^132` and `m^127`) | yes, reproduced | exponents checked, at most `Unit::MAX_EXPONENT` (60); same test |
| C4 | `wire_refusal` accepted a wire whose kinds' own types do not meet, which `validate` reports | yes, reproduced (the panel's own check hid it) | `wire_refusal` checks the kinds' types first; test `wire_refusal_also_refuses_what_the_kinds_types_refuse` |
| C5 | The committed renderer wasm lacks the badge | not a defect: the reviewer's diff left out the rebuilt assets | assets rebuilt with `graph-render/build.sh` and committed |
| C6 | `_` as a plate index generates `y[_]`, which Julia cannot read | yes | all-underscore names refused in `validate` and in RxInfer codegen; tests extended |
| C7 | `validate` accepts Julia keywords as plate indices | declined | the flow model is language-neutral; RxInfer codegen refuses keywords by name |

Mistral wrote twelve probe tests (`packages/ext-api/tests/wp37_review.rs` in its worktree, kept with its answer under `~/.cache/agent-usage/vibe/2026-11-18-2345-wp37-review/`): the parser's prefixes and edges, space containment, old files round-tripping byte-identical, two conflicting units in one net, a negative power that does not divide, a sweep comparing `wire_refusal` with `validate`, plate nesting and broadcast, subflow plate scopes, hostile unit parameters in a file, plate sizes. Its only verified finding was C1; it ran into its turn budget without a summary (stopped by hand). All twelve probes pass on the fixed code.

Found in the browser suite, not by a reviewer:
- The plate hull is the box around its members, so a plate whose members stood apart covered blocks that are not in it. A block joining a plate from outside that box now moves to its right.
- The palette placed a new block on the next grid slot even when a moved block stood there; it now takes the next free slot.
- A factor–factor wire was refused for its channels' types before the bipartite rule; the bipartite reason comes first again (gesture and `validate`).
