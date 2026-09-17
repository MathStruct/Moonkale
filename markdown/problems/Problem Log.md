---
title: "Problem Log"
tags: [problems, log]
---
Running log of problems hit during implementation. One note per problem (`P-nnn Title`), created from [[Problem Template]]. Numbers continue from the [[Problem Ranking]] where the problem was anticipated, or take the next free number.

| ID | Title | Status | Phase |
|---|---|---|---|
| [[P-001 Graph surface in desktop webview]] | Where does the wgpu surface come from inside the desktop webview? | open | 2 → 5 |
| [[P-002 Dioxus mount point and full-height layout]] | `#main` mount div broke the full-height workbench | resolved | 0 |
| [[P-003 dioxus-workbench theme variables]] | Dark palette override didn't apply | resolved | 0 |
| P-032 Release-build observability | No inspector/stdout in compiled desktop builds → file log, panic hook, JS error forwarding, Diagnostics panel ([[Debugging and Logging]]) | open | 1 |
| P-033 WebKitGTK WebGPU flag | Enable `WebGPU` feature via raw `webkit2gtk-sys` call; measure ([[Linux Desktop Setup]]) | open | 2 |
| P-034 Hydration mismatch from `cfg!` in markup | Fullstack SSR renders on the server; a `cfg!(wasm32)` placeholder showed the native text on web. Rule: no compile-time platform branches in rendered output ([[Milestone 1 - Implementation Log]]) | resolved | 1 |
| P-035 `.gitignore` ignored outside git repos | `ignore` crate defaults to `require_git(true)`; set `false` so an editor behaves the same before `git init` | resolved | 1 |
| P-036 Server-function name clash | `#[post] fn query(.., query: Query)` fails: the generated stub calls `query(...)` and the parameter shadows it; renamed with suffixes | resolved | 1 |
| P-038 Desktop link error `-lxdo` | `dx run` in `packages/desktop` fails at link time: `muda` needs `libxdo.so`; install `xdotool` (Arch) / `libxdo-dev` (Debian). Docs had it as optional — corrected in [[Linux Desktop Setup]] | resolved | 1 |
| P-037 Whole-document change events | JS reports full text per keystroke and save sends one splice; O(n) per key. Upgrade to real splices once the index needs them | open | 2 |

## Conventions
- **Status**: open · investigating · decided · resolved · wontfix.
- Link the ADR if the resolution became a decision.
- Include measurements, not adjectives ("30 fps at 50k nodes on WebGL2", not "slow").
