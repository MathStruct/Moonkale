---
title: "P-198 — Rust editor accessibility and backend activation"
tags: [problem, editors, accessibility]
status: resolved
phase: WP-03 M5
---
**Status:** resolved 2026-10-15 · **Related:** [[Rust Code Editor Roadmap]], [[ADR-0027 Simple by default]], [[Rust Code Editor Accessibility and Cutover]]

## Problem

The virtualized Rust editor exposed an empty off-screen input and a separate
visual textbox without canonical caret/selection context. Tab always indented,
so keyboard users could not leave the source. Wiki navigation required a pointer;
closing a keyboard-focused hover could lose focus. Selecting the Rust backend
also silently fell back unless its extension was enabled separately. Backend
changes did not explicitly restore the active canonical selection.

## Options

| | approach | pros | cons |
| --- | --- | --- | --- |
| A | Mirror the full document into an accessible textarea | Native editable-text API | A second full document, synchronization and large-file cost |
| B | Keep Rust input ownership; expose bounded canonical context, live positions and a keyboard focus mode | One document, bounded work, real accessibility tree coverage | Not a full editable-text document API; speech-engine behavior needs its own review |

## Measurements

Chromium's actual accessibility tree exposes one named multiline input, keyboard
help and a polite atomic status. Keyboard-only edit/history, search, rename,
hover and focus escape pass. Built-in palette/source contrast minima are 5.54:1
(light) and 4.58:1 (dark) in the fixture, with reduced motion active. Native GTK
WebKit exposes the input name/help and status text/`live=polite` through AT-SPI.
An owned bus needs `ATSPI_DBUS_IMPLEMENTATION=dbus-daemon` in this environment;
short private socket paths avoid ATK's Unix socket length limit. No user's
session bus or screen is inspected.

## Decision

Use B: retain the input sink, describe the current canonical line (at most 160
source scalars plus a leading ellipsis), caret and selection endpoints, and allow
primary-M to toggle Tab between indentation and focus movement. Primary-Enter
follows a source link or image through the existing guarded resolver. Escape from
hover returns focus. Names include the document path; focus uses theme tokens
and reduced motion suppresses animation/transitions.

Resolve the existing `editor.implementation` setting once. Selecting `native`
activates the shipped Rust extension unless explicitly disabled, without writing
migration data to the user's settings. A single release-default constant remains
`codemirror`. Restore the active UTF-16 selection when mounting the Rust view;
keep canonical document identity, dirty buffers and explicit fallback choices.

## NEXT-5 performance follow-up

The new unconditional surface focus outline caused a reproducible release-web
preview-scrollbar regression (20/21 strict, failing p95 33.335 ms). Chromium
profiling identifies synchronous DOM replacement, and a same-build rule
removal/restoration isolates this decoration. Selector-only, focus-visible and overlay changes did
not pass the first-drag gate. Reserve a transparent 2 px surface border and change
its color on focus-within; exclude the surface from generic control outlines.
Focus keeps the same visible boundary without changing layout. Each measured
drag must reset input focus/caret/viewport; repetitions without that reset hid the
initial failure in an earlier diagnostic. Keep the control focus
rules, canonical live context and selection restoration. Exact reports and final
gates are in [[Rust Code Editor Accessibility and Cutover]].

## Lesson

A hidden input is an input transport, not an accessible document model. Test the
platform accessibility tree and keyboard exits, not just labels on DOM nodes.
Report that the source context is bounded and that speech/OS-input behavior is
separate from tree semantics. Default preparation is not default acceptance.
