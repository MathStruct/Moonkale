---
title: "P-211 — Preview row queries and folding repeated full text work"
tags: [problem, editor, performance]
status: resolved
phase: WP-69
---
**Status:** resolved · **Related:** [[Rust Code Editor Implementation]], [[Rust Code Editor Roadmap]], [[Benchmarks]]

## Problem

The unchanged `live-preview-long` typing bound failed on master 3b3f0fc5 and worsened after WP-69's compact toolbar exposed more preview rows. Hiding the cursor word barely changed the profile. Restoring the toolbar's old height reduced the workload: viewport geometry and the resulting preview updates mattered.

## Measurements

Same isolated development server, Firefox, 247,882-byte Markdown note. Values are Preview / Source median milliseconds; no assertion is relaxed.

| Runtime | Preview / Source | Result |
|---|---|---|
| Master 3b3f0fc5 | 240.70 / 76.92 | fail |
| WP-69 7fd02ee0 | 556.88 / 74.68 | fail |
| Row/folding/height fixes, original development profile | 125.58 / 72.26 | fail |
| Including the prose highlighting cache, original development profile | 84.50 / 75.36 | pass (p95 87.02 / 77.90) |
| Final fresh-fixture code-only rerun, original development profile | 82.84 / 72.92 | pass (p95 86.14 / 76.20) |
| Same final code, optional development profile overrides | 73.38 / 68.70 | pass (p95 77.76 / 70.66) |

Separate Chromium CPU profiles with Wasm names identified repeated `collapsed_rows` line conversions, complete folding parses, and Markdown highlighting parses. Those diagnostic runs used a shortened navigation sequence; their absolute times are not interchangeable with the original Firefox suite. Development profile changes are measured separately and proposed as a separate commit; the code-only pass establishes the fix.

## Decision

Resolve presentation endpoints against one scalar line-start table. Row-only and zero-cell visual queries use cached wrap boundaries without copying/scanning logical lines. Retain folding parser/tree state in a three-entry thread-local cache, with UTF-8-safe edits against the exact retained source.

Keep previous measured Markdown row heights as provisional estimates while a new revision is measured. Ownership, presentation, width and font epoch still gate estimates; input/caret geometry continues to require exact revision identity. Changed content is remeasured normally.

Markdown highlighting retains existing captures for ASCII alphanumeric appends at EOF only when the prefix matches exactly, the last nonempty line is at most 4096 bytes of plain ASCII prose, and no capture touches that tail. Syntax-changing, Unicode and earlier edits parse normally, including pending prose changes. Canonical source and independently derived structural folds remain current.

The migration test separately exposed SSR hydration: wait for the existing client `/api/llm/info` handshake before submitting Explorer, and guard transiently absent status bars.

## Validation

Fresh-parser comparisons cover prose appends followed by syntax changes, Unicode, malformed fences, reference definitions and earlier edits that change context. Row-boundary tests cover indentation, Unicode, wrapping and folding. Browser coverage includes all 21 modified native suites plus 13 layout/geometry suites, and the 12 full-app suites; all pass on the code-only build. Final static checks and rustdoc pass without a Cargo patch. Workspace tests, Clippy, distribution checks and wasm are blocked by dioxus-code 0.1.2 build.rs writing to the read-only registry; Claude must rerun them outside the sandbox. The 206 code-view tests and workspace Clippy passed in preflight with an unchanged writable copy of that dependency. Per-suite results and retained evidence are in the handoff and commit request.

## Lesson

A compact toolbar changes visible preview work. Profile the derived geometry and parsing before blaming a small label, and measure algorithm changes separately from compiler-profile changes.
