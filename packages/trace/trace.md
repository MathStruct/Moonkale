---
title: "trace — implementation notes"
tags: [crate-notes, milestone-4]
---
Notes for `moonkale-trace` (Milestone 4, P-21). All platforms.

- `parse(text) -> Vec<Trace>` — Rust panics (`panicked at file:line:col` + `N: fn` / `at file:line:col` backtraces), cargo/rustc diagnostics (`error[..]:` + `--> file:line:col`), Python tracebacks (`File "…", line N, in f`), JS/Node stacks (`at f (file:line:col)`), and a fallback of bare `path:line[:col]` mentions. Frames keep listing order.
- `TraceSource::new(&trace, unique)` — in-memory `Source`, family `Custom("trace")`: root (title) → `File` nodes (key = path as printed) → `Symbol` frames (key = `path:line[:col]`, label = function or `file:line`); `Contains` edges plus `Calls` between consecutive frames. `All`/`Children`/`Neighbours`/`Node` answered; no text, no writes.
- Consumers: the terminal's **Trace → Graph** button (whole buffer via `xterm.allText`) and the Graph panel's **Trace…** paste box both call `parse` and `Workspace::add_source`; the Graph panel auto-picks trace sources and opens a frame's file at its line on double-click (`Workspace::reveal`).

Tests: `cargo test -p moonkale-trace` (each format, the fallback, the source's node/edge shape).
