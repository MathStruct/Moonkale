---
title: "P-210 — Multi-document rename needs coordinated history"
tags: [problem, editor, code-graph]
status: resolved
phase: WP-67
---
**Status:** resolved (experimental, default off) · **Related:** [[Rust Lossless Syntax and Patches]], [[P-208 Graph rename must refuse incomplete file subsets]]

## Problem

A complete rust-analyzer rename can span a declaration and consumers in several
files. Refusing the whole plan preserves safety but prevents this normal case.
Applying each file independently would leave a partial rename when one target
changes, and ordinary per-file undo would break the program again. Sources also
permit partial write success; buffer atomicity cannot promise disk atomicity.

## Options

| | approach | pros | cons |
|---|---|---|---|
| A | Keep refusing multi-file plans | safe, simple | cannot rename across modules |
| B | Stage complete buffers and coordinate marked history; save separately | supports normal rename without overwriting newer edits | needs persistent view history and bounded preparation |
| C | Rebase inverse patches over newer edits | fewer blocked undos | silently changes newer user work; complex conflict semantics |

## Measurements

Budgets: 64 files, 4096 edits, 4 MiB aggregate edit payload, 8 MiB combined
base/preview text; 512 KiB per-file input/output and 1 MiB per-file edits.
View preparation is limited to five seconds. Retained coordinated history is
bounded to 32 groups / 32 MiB. Browser probes use the committed CodeMirror bundle;
real rust-analyzer tests exercise the native and CodeMirror editors.

## Decision

B. Two rename requests discover then authoritatively plan against prepared
same-folder buffers. A newly discovered target on the second response requires
Recompute. Immutable per-file proposals validate source/document/session/version
identity. All canonical edits/history are staged and assigned synchronously.
Ctrl+Z/Y, menu and review actions coordinate opaque history markers; newer edits
block the whole operation until their local history is restored. Document close,
backend change and bounded eviction retire the group, and old markers fail closed.

CodeMirror undo transactions bypass transaction filters (`filter:false`). Both
history probes and user commands therefore inspect the command's dispatch
instead. Retain the opaque history field with its public initializer across
remounts: JSON serialization omits custom effects. Held views reject input, and
revision/sequence stamps discard queued stale messages. An authorized undo
briefly releases readOnly synchronously because CodeMirror otherwise refuses it.

Saving remains explicit and per file, continues after failure and records each
outcome. Successful writes are never rolled back automatically. Undo affects
buffers only, with dirty state relative to actual saved text.

## Lesson

Multi-buffer edits require coordinated history ownership as well as staged text.
A framework's filter is not necessarily on its history-command path; test the
actual bundled command protocol, including remounts, cancellation and redo.
