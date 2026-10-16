---
title: "P-206 — Save completion must acknowledge submitted text"
tags: [problem, editor, persistence]
status: resolved
phase: WP-67
---
**Status:** resolved · **Related:** [[Rust Lossless Syntax and Patches]]

## Problem
Workspace::save captured a text patch, awaited the source, then marked the current buffer as saved. Editing or undoing while that write was pending could make text that never reached the source appear clean. The recorded history character count also came from the newer buffer.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Lock editor input during save | simple acknowledgement | blocks normal editing |
| B | Acknowledge the submitted text and version | newer edits remain dirty | retain submitted text until completion |

## Measurements
The structural CodeMirror browser workflow exposed a save/undo timing race. A deterministic ext-api test holds the source apply future, edits the buffer, completes the save and verifies the submitted saved text/version and retained dirty newer text.

## Decision
B. Save captures the submitted text alongside the patch, updates saved/version from that snapshot after success and records its character count. Conflicts retain the previous behavior. The browser workflow waits for canonical dirty state before saving and for successful acknowledgement before its next edit.

## Lesson
Successful asynchronous persistence acknowledges the data sent, not whatever data is current when the response arrives.
