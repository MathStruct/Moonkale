---
title: "P-202 — Lossless edits must conserve syntax tokens"
tags: [problem, rust, syntax, code-graph]
status: resolved
phase: WP-67
---
**Status:** resolved in the default-off WP-67 foundation, 2026-10-16 · **Related:** [[Rust Lossless Syntax and Patches]], [[ADR-0028 Files or the graph]]

## Problem
A function moved with an EOF `//` comment can swallow following code because it has no newline. The resulting text can parse without errors despite losing a function or another item. Parser success alone is not enough to validate a lossless move. Additionally, name probes accepted forbidden Rust declaration names, and a per-edit output-size check rejected valid backward moves whose final size was unchanged.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | accept any error-free parse | simple | can silently lose syntax tokens |
| B | invent newlines/formatting at insertion | often repairs the parse | changes bytes and hides the ownership decision |
| C | reparse, conserve function count and move token inventory, reject unsafe placements | preserves the exact edit contract | some placements require a later explicit separator operation |

## Measurements
The regression fixture moves a final function/comment before a first function with no leading newline; its error-free parse loses code. The corrected planner rejects it. Tests also cover CRLF block-comment terminators, protected file prefixes, stale plans, invalid UTF-8/overlap and a valid 512 KiB backward move. Final command results are recorded in the Codex handoff; no production graph action is enabled.

## Decision
C. A planned move must retain every terminal token kind/spelling and the function count. Reject ambiguous placements rather than adding formatting. Name checks supplement the grammar with Rust's reserved/raw identifier restrictions. Validate all edits and the final output size before constructing a result.

## Lesson
Lossless printing preserves authored bytes; safe structural planning must also preserve the syntax those bytes express. Parsing, byte preservation and semantic refactoring are separate guarantees. This slice supplies pure plans, not a reference-aware Rename command.
