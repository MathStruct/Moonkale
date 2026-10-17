---
title: "P-207 — CodeMirror external edits must preserve UTF-16 boundaries"
tags: [problem, editor, unicode]
status: resolved
phase: WP-67
---
**Status:** resolved · **Related:** [[Rust Lossless Syntax and Patches]]

## Problem
CodeMirror's external replacement diff compares UTF-16 code units. A common prefix or suffix could end inside a supplementary-plane character. Changing 𐐀 to 𐐁 produced from=4/to=5 with a lone low surrogate as the inserted text. Rust cannot deserialize that string or address that position as a scalar boundary, so its mirror could become stale.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Replace every byte on each Workspace update | avoids the split | loses useful minimal-diff selection mapping |
| B | Widen the changed region over surrogate pairs and CRLF | valid protocol and minimal surrounding changes | explicit boundary handling |

## Measurements
A direct probe of the committed CodeMirror bundle reproduced the lone-surrogate splice. The structural browser suite now checks both prefix and suffix surrogate boundaries and renames supplementary-plane Rust identifiers through the real graph, remote planner, editor and save path.

## Decision
B. Workspace replacements remain one isolated undo entry; this also prevents a graph move from merging with recent user typing. The TypeScript source and rebuilt bundle are committed together.

## Lesson
UTF-16 offsets and valid Unicode strings are separate requirements. A minimal diff must respect complete scalar values at both ends before crossing the Rust/JS boundary.
