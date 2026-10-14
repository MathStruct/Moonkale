---
title: "Debugger — requirements and fit for Moonkale"
description: Someone asked for a debugger. What a debugger does (written for someone who has never used one), the requirements common to every IDE's debugger, how they would fit Moonkale through the Debug Adapter Protocol, the graph-native ideas only Moonkale can do, and a verdict on where it belongs. Study of 2026-10-14; nothing built.
tags: [research, debugger, dap, requirements]
---
**Context (2026-10-14):** a user asked for a debugger. Daniel has never used one, so this page starts with what a debugger is for. Requirements are taken from public documentation (the Debug Adapter Protocol and its main adapters), following the [[Scrutiny]] rules.

## What a debugger is for

A debugger runs your program **under control**:
- you **pause** it at a line (a breakpoint) or when something happens (an exception, a variable changing);
- you **look** at the paused state: which function called which (the call stack), and the value of every variable in each of those frames;
- you **step** one line, into a call or out of it, and watch values change;
- then you continue.

It replaces "add a `println`, rerun, read, repeat". It matters most for code you didn't write, for state that's hard to print (big structures, pointers), and for bugs that only show up after many steps. In Julia the usual tools are Debugger.jl and Infiltrator.jl (pause at an `@infiltrate` and get a REPL in that scope). VS Code's Julia extension offers a graphical one.

## Requirements every debugger UI has

`DBG-nn`. From the Debug Adapter Protocol (DAP), which VS Code, Zed, Neovim, Helix and JetBrains' adapters speak. Moonkale should speak it too, as it speaks LSP.

- **DBG-01 Launch and attach:** start a program under the debugger (configurations: program, arguments, environment, working directory), or attach to a running process (local, remote, in a container). The configurations live in the project (like `launch.json`).
- **DBG-02 Breakpoints:** by line, with a condition (`i > 100`), a hit count, or as a **logpoint** (print a message, don't stop). Also function breakpoints, exception breakpoints (on raise, uncaught only) and data breakpoints (stop when a value changes). Breakpoints persist with the project and survive edits (they move with their line).
- **DBG-03 Control:** continue, pause, step over, step into, step out, restart, stop. Run to cursor. For multi-threaded programs, per thread.
- **DBG-04 The paused state:** the call stack (frames, with source locations; library frames collapsible); variables per frame and scope (locals, arguments, globals), lazily expanded for big structures; watch expressions; hover over a name to see its value; a debug console to evaluate expressions in the paused frame.
- **DBG-05 Source mapping:** the paused line is highlighted in the editor, frames open their file, and code without source (a library, a compiled function) shows disassembly or "no source".
- **DBG-06 Output:** the program's stdout and stderr, and the debugger's own messages, kept apart.
- **DBG-07 Many languages, one UI:** an adapter per language (Rust and C/C++ through lldb or gdb (CodeLLDB), Python through debugpy, Julia through its DAP server, Go through delve, JS through the browser's or Node's). The UI doesn't care which.
- **DBG-08 Remote:** debug on the server or over SSH, the way Moonkale already runs LSP and terminals remotely ([[Remote and Server Modes]]).
- **DBG-09 Safety:** a debug session runs code. It goes through the same gate as terminals and agents on a shared server.

Less common, still asked for: **time-travel debugging** (record, then step backwards: rr, Pernosco, Undo), **hot reload** while paused, and **multi-process** sessions.

## What only a graph-native editor can add

These come out of Moonkale's model ([[Graph-Native Model]], [[North Star]]), not from DAP:
- **The call stack and the recorded trace as a graph:** frames as nodes, calls as edges, drawn in the graph view. A recorded run (time travel) becomes a graph of calls over time. Moonkale already turns stack traces from the terminal into graphs ([[LSP and Terminal]]); a debugger is the live version.
- **Breakpoints and watches are annotations** on code nodes ([[Annotations]]), so they survive refactors that move code (content-addressed identity), not just line moves.
- **Variables as graphs:** a paused value that is a structure with references (a linked list, a tree, an object graph) is drawn as a graph, not only as a nested list.
- **A model debugger:** for factor graphs, the "program" is a message schedule. Stepping, breaking on a condition (a NaN, divergence), and inspecting a message are exactly debugger operations, applied to a model ([[Lenticulum Frontend Requirements]] LT-19). That is the debugger Lenticulum needs, and the one that fits Moonkale best.

## Fit and verdict

- **It fits, as an optional extension**, not core:
  - a `dap` service beside `lsp` (spawn and supervise adapters, local or remote, one session type);
  - a debug panel (call stack, variables, watches, console, breakpoints);
  - editor integration (gutter breakpoints, the paused-line highlight, hovers) through the code editor's decoration API.

  Desktop and server only (adapters are native processes); the web client debugs through the server.
- **Order:** the code-debugger extension can wait. The model debugger (LT-19) comes first, because it serves the reason Moonkale exists and reuses the graph engine and the flow editor. The DAP extension then reuses its panel layout.
- **Rejected or limited** (for [[Scrutiny]] when built): VS Code's `launch.json` variable substitution language as is (Moonkale's settings are layered and typed, so configurations are typed settings); a debugger that pauses the whole UI (sessions are async, like LSP).

Work package: WP-39 in [[Work Packages]] (the DAP extension, after the model debugger).
