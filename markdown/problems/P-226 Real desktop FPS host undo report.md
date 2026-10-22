---
title: "P-226 — Real desktop FPS host undo report"
tags: [problem, editors, testing]
status: resolved
phase: WP-02
---
**Status:** resolved · **Related:** [[Rust Code Editor Roadmap]]

## Problem
Daniel reported that typing in the release native FPS host followed by Ctrl+Z
did nothing. Synthetic renderer-probe undo already passed. The release binary
in `~/.cache/moonkale-fps-real` must remain untouched.

## Options
| | approach | pros | cons |
|---|---|---|---|
| A | Reproduce with compositor keys and canonical text observation | Distinguishes input delivery, focus and history | Private sway/Mesa differs from KDE/NVIDIA |
| B | Remove workspace/session divergence reconciliation | None without evidence | Risks applying stale history over external edits |

## Measurements
A fresh native build on the private Wayland display receives trusted Ctrl+Z
(`key=z`, `code=KeyZ`, Ctrl set) and removes the final typed character, also
while recording. Native viewport key handling calls the same `history` callback
as shell `Command::Undo`, directly; it does not depend on shell dispatch.
The divergence guard is not reached for these ordinary local edits.
`native-editor-fps-input.py` now passes source, documentation preview and a
3 MiB single-line document: real typing, one-character undo/redo and selected
replacement undo all match exact canonical text. Its six native recorder
checks also pass interaction validation (112 arrow keys or 63 small viewport
movements per case). Reports are under `target/next8/os-input*`; these are
development correctness runs with observers, not FPS acceptance.

## Cause
Cause (2026-10-22): Daniel's KDE session had the US layout (`~/.config/kxkbrc` `LayoutList=us`, system layout `de`) on a German keyboard, so the key labelled Z sent Y and Ctrl+"Z" was redo. With the session on `de`, Ctrl+Z undoes (checked in Kate). Not an editor bug. The same mismatch made Daniel's password fail on the lock screen (session layout) but not at login (system layout).

## Decision
Keep the external-edit guard. Add an OS-input regression and opt-in FPS-host
canonical-text/passive-event observers to retain reproducible evidence. Do not
claim that Daniel's real-session failure is fixed without reproducing it.

## Lesson
A passing synthetic key test does not prove native delivery; a passing private
Wayland test likewise does not prove the user's KDE session or keyboard layout.

A "shortcut does nothing" report from a real session: check the active keyboard layout against the physical keyboard first.
