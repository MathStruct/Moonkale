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
| P-039 No native folder dialog in Milestone 1 | By design (out of scope); now `rfd` (xdg-portal backend) behind `WorkspaceConfig::pick_folder` on desktop, wired to File → Open Folder…, Ctrl+O and the Explorer button. Requires `xdg-desktop-portal` + a backend on Linux ([[Linux Desktop Setup]]) | resolved | 1 |
| P-040 Custom title bar on an undecorated window | Decorations off; drag/resize/minimize/maximize/close via `dioxus::desktop::window()` callbacks passed into `ui::Frame`. Maximize icon doesn't flip; Wayland resize depends on the compositor | resolved (gaps noted) | 1 |
| P-041 u64 through JSON loses precision | `Version(u64)` hashes > 2⁵³ became floats on the `BroadcastChannel` path and failed to deserialize, silently killing the receiver loop. `Version` now serialises as a hex string; receiver loops skip bad messages instead of exiting | resolved | 1 |
| P-042 `Moved` echo check used the wrong field | "ignore my own messages" keyed on `from`, but `Moved.from` is the origin, not the sender — the origin never closed its copy. `SessionMessage::sender()` returns `to` for `Moved` | resolved | 1 |
| P-043 dx serve missed a Rust change | A change in `ext-api` was not rebuilt by a long-running `dx serve` (no rebuild line in the log); restart dx when a non-rsx change seems not to apply | open (tooling) | 1 |
| [[P-045 Cross-window drag and drop]] | Firefox "Server Not Found: wb-tab-editor-…" (unhandled tab drop navigates); desktop drag + folder sync unverified. Tab drag is now the gesture, stray drops inert, desktop seeds sources from the registry, session log + "N windows" status for diagnosis | resolved (desktop confirmed by hand) | 1 |
| P-044 Cross-window drag didn't work by hand | Three causes: (1) Firefox never *starts* an HTML5 drag unless `dragstart` calls `dataTransfer.setData` — Dioxus's synthetic handler can't, so a native listener on the handle now does; (2) web *New Window* opened a tab, and a background tab can't be a drop target — now a popup window; (3) whether WebKitGTK delivers an HTML5 drop across two webviews of one process is unverified, so the offer now survives `dragend` as a **"Move it here" banner** in the other window (grab → release → click). Verified on web with a real mouse drag; desktop still needs a manual run | resolved (fallback) | 1 |
| P-046 Random element ids vs hydration | Graph panel ids derived from the random `WindowId` differed between server render and client; `getElementById` failed silently. Deterministic ids for anything an eval looks up ([[Milestone 2 - Implementation Log]]) | resolved | 2 |
| P-047 Eval messages lost | Sent-before-awaited and sent-then-returned messages are dropped; also an eval mounted from an effect that writes a signal it reads. Pattern: `init` handshake, park on `recv` until `destroy`, mount from `onmounted` | resolved | 2 |
| P-048 dx serve misses Rust changes (3×) | Non-rsx changes to a `const &str` script were not rebuilt; restart dx each time (~3 min) | open (tooling) | 2 |
| P-049 No GL in headless Firefox; wgpu hangs without a context | Probe WebGL2/WebGPU before `create()`, show the reason; rendering verified only in real browsers | resolved (fallback) | 2 |
| P-050 wgpu 30 API drift | Several descriptor fields/enums changed; read the registry source | resolved | 2 |
| P-037 Whole-document change events | JS reports full text per keystroke and save sends one splice; O(n) per key. Upgrade to real splices once the index needs them | open | 2 |

## Conventions
- **Status**: open · investigating · decided · resolved · wontfix.
- Link the ADR if the resolution became a decision.
- Include measurements, not adjectives ("30 fps at 50k nodes on WebGL2", not "slow").
