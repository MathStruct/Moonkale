---
title: "P-245 — Factor subsystem review findings"
tags: [problem, flow, rxinfer, julia]
status: resolved
phase: WP-08
---
**Status:** resolved before merge · **Related:** [[ADR-0022 Bipartite factor graphs]], [[Flow Editor]]

## Problem

Claude's WP-08 (ADR-0022 stage C, 2026-11-18) went to Mistral (`review-rust`, headless) twice before commit: the RxInfer codegen in moonkale-julia, then the ext-api and panel diff. Every command was refused in both runs (the approval callback denied even allow-listed `cargo` and `git`), so both reviews are static traces; each finding below was checked by a test written first.

## Triage (2026-11-18)

| # | Finding (reviewer) | Right? | Done |
|---|---|---|---|
| V1 | A boundary pin open outside, with an inner variable wired to it, generates: the inner variable stands in for the outer one (`y[i] ~ Bernoulli(coin_1_v)`) | yes, reproduced | codegen refuses any subsystem pin wired inside but not outside; test `review_findings_are_refused` |
| V2 | A wire to a port the block lacks (`flip.bogus`) is dropped silently: the channel falls back to its constant | yes, reproduced (also before stage C) | a wire end must name a pin of its kind; same test |
| S1 | Duplicate block ids in one scope collapse | not a stage-C defect | `validate` and the panel never create them; left |
| S2 | `is_ident` misses Julia keywords (`while`, `else`, …) | yes (old) | all reserved words refused; same test |
| F1, F2 | The wire gesture is not tested for an inner variable on a boundary pin, or a factor subsystem onto a factor (`validate` is) | missing tests, behaviour right (the gesture resolves through `inner_ports`) | both added to the panel test |
| F3, F4 | The palette's role choice and a role-less subflow's `boundary_role` untested | partly | `boundary_role() == None` asserted; the palette path is covered by the pure `subsystem_role` test |
