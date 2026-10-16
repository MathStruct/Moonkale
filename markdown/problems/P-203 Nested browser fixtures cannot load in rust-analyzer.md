---
title: "P-203 — Nested browser fixtures cannot load in rust-analyzer"
tags: [problem, testing, lsp]
status: resolved
phase: WP-67 verification
---
**Status:** resolved, 2026-10-16 · **Related:** [[How to Write Tests]], [[Rust Lossless Syntax and Patches]]

## Problem
Running the ordinary browser fixture under this worktree's `target/wp67-e2e/m2root` made lsp2 wait 120 seconds for diagnostics. `cargo metadata --manifest-path …/m2root/Cargo.toml` failed because Cargo discovered Moonkale's parent workspace but the disposable package was not a member. The fixture worked outside the repository, hiding the problem.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | move every fixture outside the repository | avoids discovery | contradicts worktree-local artifact organization and needs more sandbox paths |
| B | add the disposable fixture to Moonkale's workspace | metadata works | changes the product workspace for test data |
| C | give the generated fixture an empty `[workspace]` table | fixture is self-contained in any location | none for this isolated test package |

## Measurements
The failed metadata command names both the fixture and parent Cargo.toml and recommends an empty `[workspace]`. The initial browser batch's five non-LSP suites passed; lsp2 alone failed at server readiness. The corrected fixture and rerun are recorded in the Codex handoff; the failed first log is preserved.

## Decision
C, in the shared `fixture.sh` template, before its fixture commit is created so run-all resets preserve it. No change to product workspace membership, runtime LSP handling, assertions or timeouts. Test builds remain in this worktree's target directory.

## Lesson
A disposable Rust project should declare its own workspace rather than depending on where the test runner puts it. Language-server readiness failures can originate in Cargo metadata rather than editor behavior.
