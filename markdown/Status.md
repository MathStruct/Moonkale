---
title: "Status"
description: What Moonkale can do today, area by area — built, partly built, designed only — checked against the code on 2026-10-01 (after Milestone 17), with the open problems that matter most. The one place where status is kept.
tags: [status, moc]
---
**This page is the single place for "where are we".** The landing page, [[Home]], the README and the [[Roadmap]] link here instead of repeating it. Update it at the end of every milestone. Checked against the code on **2026-10-01** (`refactor` branch = `master` + Milestone 17).

Legend: ✅ built and tested · ◐ partly built (the missing part is named) · 📝 designed only, no code · ⚠ built but broken

## Sophia save warnings and Git everyday (validated 2026-10-29)

✅ Built and validated in the codex contributor worktree. Sophia v0.1.8
successful-save warnings reach a localized status notice with the full message
on hover. P-238 retains exact fetched patch bases when references change the
printed text; concurrent definition/group edits still conflict.

WP-52 adds hunk/line staging and unstaging, including new/deleted text files,
reviewed branches/stash/reset/revert/cherry-pick, reflog branch recovery, and
shared Rust/CodeMirror blame gutters with historical walking. Sync stays
primary; secondary flows use the overflow and palette. Destructive reviews
name affected commits/paths, ignored-file switches are refused, and automatic
maintenance/pruning is disabled. P-237 fixes untracked-stash cleanup.

All AGENTS gates and the reconciled 59-case browser feature matrix pass.
There are 24 Git tests, 17 Sophia tests and 28 standalone parser regressions;
the final Git parser fuzz run completes 1,141,732 inputs without a crash.
See [[Version Control Requirements]] and [[Security]] for scope and bounds.

## Agent display and private terminals (WP-70 / WP-71, 2026-10-29)

✅ Built and validated on the codex contributor branch. Typed
agent reveal/select/result/canvas tools use marked groups, Ask/When idle/Always
user/session settings and per-conversation rate/budget bounds. Both terminal
views detect Unix canonical echo-off prompts, confirm pastes and suppress shared
tails, with per-terminal Pause sharing. See [[Agents in the UI and on the Graph]]
and [[Security]]; the raw-mode/non-Unix limitations remain explicit. All AGENTS gates
and the smoke, structural, Sophia and default graph browser matrix pass. Shared
control and global Stop sharing remain WP-72.

## Overview (WP-60, checked 2026-10-29)

✅ Steps 1–4 are built and validated: grouped open-tab cards, effective
Ctrl+Alt+Space/F13 shortcuts, keyboard/mouse switching and close, bounded lazy
extension previews, decoded-image reuse and graph/flow engine snapshots.
Title-bar search and the overview share the existing file/command list.
Both journeys cover overview tab switching; relay previews strip terminal
control sequences before text bounding. P-236 fixes dirty-close confirmation
focus and scoped Escape cancellation; five overview repetitions, three full
smoke sweeps and all gates pass with the fix.
The browser measured 14.63 ms for twelve tabs/two tiles and 23.25 ms for fifty
tabs. All AGENTS gates, the complete browser feature matrix, the image regression
and an inspected private desktop check pass. Other-window switching, docking,
hot corner and animation remain step 5. See [[Overview]] for scope and bounds.

## Linked panels (WP-59, checked 2026-10-29)

✅ Session-owned groups, marked tabs, independent local graph instances and
Open with context are built and validated on the contributor branch. Keyboard
and mouse routes, group steering/Back, selection sync, pin/unlink and close
rules are included. Model/migration and session tests, browser acceptance,
private desktop inspection, all AGENTS gates and the full CI browser feature
matrix pass; see [[Linked Panels]]. Step 6 (other kinds of follower pairs)
remains later work.

## Sessions (WP-46, checked 2026-10-28)

Named sessions live in the state store and own source/tab/layout metadata,
recovery, conversations and settings between user and folder scope. Existing
rows migrate to Default once. The title bar and palette open a keyboard/mouse
switcher with new/rename, confirmed state-only deletion and versioned JSON
metadata export/import. Secret references travel by name; buffer/conversation
bodies stay private. Live switches retain dirty documents, editor models and
non-text tabs. Default preserves each platform's startup behavior.

All AGENTS gates, the complete CI feature matrix, sessions/recovery and extra
conversation/settings/window/history regressions pass. A private desktop check
switches two different dirty folders without changing their files. The import
parser completed 3,712,726 fuzz inputs without a crash. See [[ADR-0025 Sessions are the outermost state]] for shipped scope and later ADR operations.

## Buffer recovery and cache limits (WP-56, 2026-10-28)

Unsaved folder buffers are sampled into the existing client state store every
two seconds and on blur/switch, with per-buffer/total bounds and secret-path
exclusions, checked again by the folder source against ignores and symlinks.
Startup offers one quiet notice with per-file Restore/Discard;
stale bases show a comparison. Saving, explicit reload and confirmed close
remove snapshots. Settings lists platform cache limits. Existing Sophia
input-hash manifests reuse unchanged code on reopen; content identity, math
and image caches retain bounded reuse. See [[ADR-0026 Memory and the store]]
for limits and remaining persistent-preview/Android-pressure follow-ups.
Validated with all AGENTS gates, the full CI browser matrix, snapshot/cache
regressions and a private-desktop kill → restore → save check on 2026-10-28.

## Interface density (WP-63, 2026-10-28)

Simple is the default; Appearance and the palette switch to Detailed. Simple
folds the menu bar, limits the rail to seven entries and status to five items,
and puts secondary panel actions in flat ⋯ menus. Optional extension chrome is
a saved user choice in Extensions; its commands and menus remain available.
Existing users keep their enabled extensions' chrome through one-time migration.
Detailed shows the full surface without changing capabilities or keymaps.
All AGENTS gates and the complete CI browser matrix pass, including targeted
reruns after selector fixes. `density.mjs` checks 62 commands in both densities,
legacy migration and the Detailed Rust toolbar at phone width. Both densities’
screenshots were inspected; validation details are in `HANDOFF-from-codex.md`.

## Numbers
40 crates · 46 900 lines of Rust · ~1 000 lines of TypeScript in four view bundles · 146 Rust tests · 47 browser suites (Playwright) · CI since 2026-10-01: fmt, layering rules, tests, clippy, wasm check, six browser suites (R-39) · releases `YYMMDD-proto` for Arch, Debian, Nix, tarball, Windows, macOS, Android ([[Install]]).

## Security CI (WP-47, checked 2026-10-17)

Blocking all-feature Rust licence/source/advisory checks, cargo-audit, six JS
lockfile audits and offline SHA-256 checks for 63 vendored files are implemented.
Existing Rust/npm findings are dated exceptions, awaiting Daniel's upgrade
choices; they are not fixed. Two separate non-blocking nightly parser fuzz jobs
run for 60 seconds each. Local runs completed 2,140,753 math and 8,218,925 flow
inputs without crashes; 18 parser tests and nine gate regression tests pass.
The first CI math crash (P-212) now has source-preserving production guards and
four corpus regressions; 216 code-view and five agent unit tests pass (code-view
uses the supported `DOCS_RS=1` registry-write bypass); browser verification remains due. The math normaliser remains planned; the target covers today's math range parser.
See [[Security]] and [[How to Write Tests]] for scope and commands. The page's
older whole-product inventory date above is unchanged; this paragraph checks only
WP-47, not every area below.

## Input layouts (WP-61, checked 2026-10-28)

Moonkale, VS Code, Obsidian and Left hand + mouse presets are available in
Settings → Keybindings. Explicit bindings and unbindings stay above each
preset; conflicts are marked. Tooltips, key tips and the cheat sheet use the
same effective registry. Punctuation follows physical US key positions and
uses platform layout labels when available; letters remain logical. F13–F24
are bindable, with defaults for existing actions.

Mouse thumb buttons navigate a bounded window-local history of foreground
documents and graph-camera gestures. The middle button opens supported targets
in background tabs, closes tabs and pans canvases; mappings are configurable
beside keybindings. Alt+Enter in Go to File and the Explorer menu also open in
background. The left-hand preset adds focused canvas/table controls, including
camera-plane pan and dolly in 3D. F1 lists the active preset and mouse actions.
New mouse/keymap suites, the full smoke/structural/Sophia/default-graph matrix
and all AGENTS gates pass. Real OS layout/IME, WebKitGTK and Android input checks
remain manual; reserved keys for unbuilt overview/link-group/proposal/comment
features stay unbound. See [[Input Layouts]] for shipped scope and later work.

## Shortcut discovery (WP-64, checked 2026-10-28)

Command tooltips, menu entries, palette rows and empty-tile hints now share the
resolved keymap, including rebinding and unbinding. Holding Ctrl/Cmd alone for
500 ms shows shortcut badges on visible command buttons; text inputs
and editors suppress them. The web's Open Folder command focuses the Sources
path field. The keyboard journey covers folder/file open, edit/save, tabs,
Graph and closing editors. F1, the palette and Help open one grouped,
searchable cheat sheet with effective bindings. Mouse commands get one quiet
shortcut hint on their third use in a day, remembered in the platform state
store; Settings can disable hints. Printable keys and `/` open the title-bar
file/command search outside text owners, preserving graph navigation.
Command-opened panels and dialogs take focus and return it on closure.
Both interaction journeys, the discovery/focus suite, both-editor structural
matrix, Sophia and the default graph suites pass; the hold-Ctrl screenshot is
recorded. Keymap presets and five-button mouse mapping are covered by WP-61 above.
See [[ADR-0027 Simple by default]].

## Crash safety (WP-57, checked 2026-10-28)

Native built-in extension callbacks, tasks, effects and resources now have
per-extension panic guards; a failure removes that extension's contributions
and shows one notice. The web's aborting wasm client has an independent reload /
local-copy / safe-mode overlay. Repeated incomplete startups offer safe mode;
desktop `--safe` and web `?safe` ignore extensions and saved layout without
changing preferences. Both wasm runtimes already created fresh instances and
now bound retries to three after the initial failed invocation. Unsaved-buffer
snapshots and compiled service hot reload remain unbuilt. External compiled-in
extensions must adopt the new callback/task helpers. P-234 fixes a Chromium
structural-preview crash by heap-pinning futures before constructing async
guard wrappers. See [[Crash Safety]].

## Cross-file code dependencies (WP-22, checked 2026-10-28)

Rust, Python and Julia Calls/References now resolve explicit folder imports to
unique indexed definitions, with import provenance and occurrence ranges.
A batch refresh updates incoming edges when targets change, disappear or become
ambiguous. Sophia persists these as file-owned annotations and projects them
into cache graph reads; no-op reopen and content identity remain unchanged.
Globs, external targets, dispatch and unsupported import environments remain
unresolved. See [[Indexing]] for exact syntax, bounds and coverage.

## Portable CI (WP-26, checked 2026-10-17)

Windows and Apple-silicon macOS now have an advisory `portable` matrix for core,
folder, index, extension API, LLM, Unison and remote library tests, followed by the
default extension distribution check. Native runs are pending GitHub validation;
UI, installers and Intel macOS are outside this CI job. See [[How to Write Tests]].

## First Windows UI findings (P-217–P-219, checked 2026-10-22)

Responsive layout changes retain each desktop tab group's selection and ignore
viewports smaller than 100 pixels on either axis. Rich Markdown images resolve
through the document's source and retain relative paths when saved. CodeMirror
find/replace uses theme tokens and English/German/Chinese locale phrases.
VirtualDom and browser regressions cover these changes, with packaged Linux/WebKit
private-display checks of Graph retention, Rich images and the German find bar.
Windows recheck remains
Daniel's. See [[Problem Log]], [[Markdown and Typst Editor]] and [[Code Editor]].

## Canvas components (WP-04/05, checked 2026-10-23)

The engine reports one changed camera mapping per frame and accepts embedded
rectangles without label/symbol ink. A shared Dioxus `canvas-layer` crate mounts
world-space cards over the two canvas surfaces, culls small/off-screen/3D cards,
and isolates component clicks. Flow's parameter editor uses it, follows the
camera and offers header dragging with one undo step. Automatic flow fits leave
room for the card; explicit zoom remains free. Extension-provided node-kind views
remain planned. See the Components on the canvas entry in [[specifications/README|Specifications]].

## E-graph drawing (WP-07, checked 2026-10-23)

The Graph panel's paste dialog accepts plain engine e-graph JSON: e-classes as
anchor-backed groups, operators with child edges to classes, and labelled
rewrite provenance. Per-class/all-class collapse and expand use existing
aggregate/proxy deltas. Provenance can be hidden; selecting an operator
highlights incoming rewrite ancestry. The synthetic `(a*2)/2 → a` fixture and
engine/panel browser suites cover this path. Input is an in-memory bounded
snapshot; JSON file opening, editing and live Sophia production remain open.
See [[Graph View]] and [[North Star]] for the producer contract.

## Coarse graph queries and navigation (WP-06, checked 2026-10-23)

Hierarchy sources expose bounded coarse cuts, counts, weighted relations and
region members. The graph toolbar chooses a tier at open/Reload, shows it and
offers a per-source override. One region expands/collapses through the existing
aggregate delta path, with pinned context and a stable camera. Core's fully
generated million-node native projection takes about 1.32 seconds; the separate
lazy million-node browser fixture checks the production panel. Sophia
copy queries take 3.68 s cold / 1.81–1.83 s warm for 199,257 nodes → 2; cached source cuts/layouts, upstream full traversal, nested navigation and persistent
workspace overrides remain open. See [[Case Selector]] and [[Graph View]].

## Flow symbols and typed cells (WP-11/15, checked 2026-10-23)

Closed absolute symbol subpaths fill in block colour. Flow grid settings govern
insertion, Layout and dragging, including card headers; Alt bypasses snapping.
Core now models UTC date/time cells, labelled node references and bounded finite
vectors. SQL date metadata and DuckDB numeric lists feed localized date display
and theme-coloured sparklines; index search paths carry node references.
Unit/source/grid tests and the flow/SQL browser suites pass, including real
MTK/RxInfer simulations, fill pixels, saved grid positions, locales and reference
activation. Both parsers have bounded fuzz targets. See [[Flow Editor]] and
[[Table Editor]] for conversion limits.

## Content identity nesting (SR-145, checked 2026-10-24)

Content-id syntax, binding and scope passes use explicit heap stacks with total
byte/work budgets, replacing the depth-128 refusal. Normalizer v7 and golden ids
remain unchanged. Julia's `modules_around` and `locate` helpers are exported for
Sophia. See `packages/services/content-id/content-id.md` for limits and checks.

## Shared code naming (WP-21 prerequisite, checked 2026-10-23)

`moonkale-content-id` now exposes shared binding/entity-path metadata and method
grouping for Julia, Rust and Python. Julia ownership is ported from Sophia
`773d9898`; Rust impl/trait paths and Python class paths use caller-supplied root
context. Unknown roots are flagged, invalid names reported, and naming golden
data complements the unchanged v7 identity fixtures. The per-folder Sophia writer is implemented (WP-21, 2026-10-24):
manifest-hash reuse, atomic watcher batches, bounded conflict retries and
a read-only additional Source with current-file fallback on stale text.
Definition ids rebuild from files; generic nominal ids may change on rebuild. See [[Code Graph Schema]] and [[Work Packages]].


## Sophia folder cache (WP-21 / ADR-0030, checked 2026-10-27)

Default-on `sophia` on desktop/mobile, implied by the server feature, writes admitted Rust/Julia/Python code into the derived
`.moonkale/code.sophia` cache and opens it as an additional read-only Source.
Reopen skips unchanged files; one watcher window is one transactional batch,
including moves and bounded name-owner retries. Stale reads fall back to current
files. Legacy code graph reads now default to the cache; search remains in memory.
The 2026-10-27 verification passes 867 workspace tests (seven ignored), 81
focused index/Sophia tests including 16 folder-cache regressions. Eight browser
suites cover default cache reads, persisted override, visible fallback, repair,
watch updates and ordinary graph/search/settings/palette behavior. See [[Indexing]]
for the passing mandatory gates and desktop/server/mobile/Android compilation
checks. The final defaults-only Sophia and Local browser checks also pass.
The setting `code_graph.source = auto | in-memory` and the status-bar backend
item expose the choice and fallback reason. A corrupt cache gets one rebuild
attempt; **Check the cache** compares a fresh temporary store with the replay
digest, repairs differences and logs/notifies the problem. Browser-only builds
keep the in-memory path or use their server.

Native debug measurement on Moonkale: **8.830 s cold / 0.664 s reopen**, including
new folder snapshots; store work alone 8.101 s / 0.030 s. **36,704,256 bytes** for
478 files / 6,111,526 source bytes. Reopen performs zero parses or replacements (timings rounded independently).
The normal form refuses 2,787 definitions and reports 12 ambiguous named
bindings; this is partial syntactic coverage. See [[Indexing]] for methodology,
module-naming limits and evidence. ADR-0029's four default gates are met;
ADR-0030 authorizes this legacy-folder default.

## Folder truth (ADR-0030, checked 2026-10-27)

Folders detect the bounded `.moonkale/truth.json` marker on open and carry
`truth: files | store` in folder/index descriptors. The status bar and explorer
root label the truth in English, German and Chinese. The status bar shows only
the active document’s or selected Explorer folder’s truth and code-graph backend
(first folder when neither applies); its tooltip lists every folder and any
fallback or read-only reason. Marked folders use the
in-memory code graph with an explicit read-only explanation until SPEC-010
lands. Plain file editing/search remain available; code-store writes, cache
checks and repairs are refused, preserving even unreadable stores and SQLite
sidecars. No command creates a marker or moves the truth yet. Generated files,
outside-edit proposals and New project remain planned. See [[Indexing]] and
[[ADR-0030 Where the truth lives]]. All AGENTS gates pass (871 workspace tests,
seven ignored); the focused folder/index/Sophia run passes 118 tests and the
`sophia-folder` browser suite checks both labels and the read-only explanation.

## By area

| area | state | notes |
|---|---|---|
| **Workbench** (docking, rail, menus, palette, quick open, keybindings, phone shell) | ✅ | [[ADR-0010 dioxus-workbench for layout]], spec [[009]], [[011]] |
| **Folders** (open several, watch disk, file ops, trash, find/replace) | ✅ | Milestone 7, 16; WP-23 (2026-10-17): metadata-only listings with separate fetched content versions, locked/rechecked saves, consistent directory versions, bounded directional neighbourhoods and native no-replace moves; final external-writer race remains #15; symlink swaps remain #5 |
| **Remote folders over SSH**, desktop as a server's client | ✅ | Milestone 11; hardening #18 |
| **Unison codebase source** | ✅ read-only, default-off | WP-18 (2026-10-17): local UCM 1.5 HTTP snapshot, store truth, content-addressed definitions/names/dependencies, hermetic schema tests and native graph-renderer payload test; `unison` feature, `*.unison` root opener; bounded snapshots, reopen for updates; see `packages/sources/unison/sources-unison.md` |
| **Sophia code store source** | ◐ default-on in apps/server | WP-68 (2026-10-20): `*.sophia` stores through Sophia's `sophia-interface` (git rev): store truth, names/definitions/namespaces, text edits that move names, parse diagnostics, polling change feed, rendering profile as a `sophia:Settings` node, read-only `sql` and Sophia's `query` (table panel switch, views as tables, 2026-10-21); SPEC-004 §9 tests through `Source`. Missing: annotation edits, typings (Sophia M4); see `packages/sources/sophia/sources-sophia.md` |
| **Projects** (saved sets of sources, selector, sync) | 📝 | [[Projects and Sources]], spec [[003]] |
| **Code editor** — CodeMirror (LSP, highlighting, wiki-links, presence) | ✅ | splices (spec [[018]]); full-text LSP sync (#19) |
| **Code editor** — Rust (editor-core + Dioxus, opt-in) | ◐ | WP-69 (2026-10-17) compact icon toolbar (Save/Reload/Wrap/⋯), indentation settings in its gear popover, inline cursor word hidden on phones; preview typing regression fixed (P-211: 83 ms Preview / 73 ms Source with code fixes alone in the local development browser check); revisioned edits/history, highlighting, live browser/native WebKit LSP integration, browser/native AT-SPI accessibility semantics, tested backend migration, search/folds, Markdown preview and default-off Rust/Python/Julia documentation previews; latest strict release web frames 21/21 after the focus-border fix (prior preview-scrollbar failure retained); Rust highlight CPU time reduced, desktop software-GL timing still fails with quantized clocks; default prepared but unflipped; 2026-10-22: native FPS idle/input accounting corrected (P-225), session-specific undo under investigation (P-226); real-desktop frames and Android acceptance remain open — [[Rust Code Editor Implementation]], [[Rust Code Editor Roadmap]] |
| **Markdown** — rich (Milkdown), source, wiki-links, KaTeX, front matter, typography | ✅ | specs 012, 013, 019, 021, 026 |
| **Typst preview** | ✅ | no packages |
| **Diagram fences** (tabs, Typst math, Mermaid, TikZ) | 📝 | [[Markdown Diagrams and Math]] |
| **Graph view** (wgpu 2D/3D, Barnes–Hut, Local mode, several folders) | ✅ | 100k nodes on the CPU; 2026-10-22: overview drags pan below a 3 CSS px radius (P-223); private WebKit desktop initial picture 6.78 → 2.54 s, warm median 5.70 → 2.63 s (P-224; physical NVIDIA recheck due), see `packages/graph-render/graph-render.md`; WP-06 coarse queries, selector and one-region expansion built (1M fixture first picture 110 ms); cached cuts/layouts and upstream Sophia full traversal remain open ([[Case Selector]]) |
| **Flow editor** + Lux.jl library | ◐ | editing and codegen; running the model is not built |
| **Table editor** | ✅ | read-only |
| **Image viewer** | ✅ | spec [[008]] |
| **Terminal** — xterm and Rust | ✅ | traces become graphs |
| **Notebook editor** | 📝 | [[Notebook Editor]] |
| **Agent** (sessions, saved agents, policy gate, MCP, server sessions, Claude Code) | ✅ | [[Agent Sessions and Profiles]]; wedges (#13) |
| **Search** (BM25 + embeddings) | ✅ | in memory, rebuilt on open |
| **Index**: wiki-links, Rust definitions | ✅ | WP-23: failed text reads carry a content-free node diagnostic, cleared on recovery; WP-65: shared L2 kinds, stable location ids, ranged occurrences, names and provenance; index rows rechecked 2026-10-15 — [[Code Graph Schema]] |
| **Index**: Julia, Python definitions | ✅ | WP-65: same shared schema; Julia overload ids use signatures rather than order |
| **Index**: Rust/Julia/Python calls, references and imports | ◐ | WP-22: conservative same-file syntax resolution, explicit unresolved targets; compiler-backed dispatch and cross-file resolution remain open — [[Code Graph Schema]] |
| **Index**: content identities (Rust/Julia/Python) | ◐ | WP-66: default-off explicit requests, versioned syntactic normalization, canonical recursion components, bounded caches and freshness guards; conservative syntax subset and same-file resolution, not semantic equivalence. Normal form and hashing are the crate `moonkale-content-id`, shared with Sophia, with golden fixtures; normalizer v7 (2026-10-24) — [[Code Graph Schema]] |
| **Code graph**: Rust lossless syntax and patch planning | ◐ | WP-67 slices 1–5 (2026-10-16): exact Rust plans and a revision-aware adapter, plus default-off structural-edits graph controls for declaration names and same-file moves on desktop/web; previews, grouped undo and explicit conflicts built; complete same-folder rename uses rust-analyzer with coordinated keyboard/menu/review history and explicit per-file saves; cross-file moves remain open — [[Rust Lossless Syntax and Patches]] |
| **Index**: other languages | 📝 | [[Core Languages]] |
| **Git** (changes, diff, stage, commit, history graph, sync, text conflicts) | ✅ | WP-51 (2026-10-29): fetch/configured pull/push, exact-commit lease, publish/upstream, live progress, server checks and system credentials; reusable three-way text hunks with edits, stale-save refusal, continue/abort for merge/rebase/cherry-pick, including delete/modify resolution. Twelve Git/model tests, all gates, 50 browser cases and private desktop acceptance pass. Binary/mode conflicts require external Git; details in the crate notes |
| **History** (entity log, restore, compaction) | ✅ | one row per event in the folder host's state store (Milestone 18 phase 5.10; was JSONL); #17, P-086 |
| **Presence** (who's here, cursor lines, desktop hub client) | ✅ | |
| **Shared editing** (CRDT) | 📝 | [[Collaboration]] |
| **Annotations** | 📝 | [[Annotations]] |
| **Unicode input** (`\int` → ∫) | 📝 | [[Unicode Input]], spec [[004]] |
| **Server** (`moonkale-server`: token, TLS, jail, audit, terminal switch) | ◐ | single token, no accounts; eleven open security findings ([[Audit 2026-09-23]]) |
| **Reader mode** (published, queryable sources) | 📝 | [[Publishing Sources]] |
| **Extensions — static** (Rust crates, tiers, permissions, settings per extension) | ✅ | the catalogue is hard-coded in `ui` ([[Extension Catalogue]]) |
| **Extensions — wasm** (JSON ABI v1, commands/tools; wasmtime + browser Worker) | ◐ | no UI contributions, no manifest file, no budgets — [[ADR-0013 JSON ABI before components]] |
| **Extension manifest, activation events, `when` clauses, contribution points beyond panels/commands** | 📝 | [[Contribution Points]], [[Manifest Reference]], [[Host API Reference]] describe the target |

## Sources

| source | state | platforms |
|---|---|---|
| Folder | ✅ watched | all (Android: private storage only, spec [[028]]) |
| SQLite, DuckDB (+ CSV/TSV/Parquet folders) | ✅ read-only | desktop, server |
| Turso, redb, RocksDB, HelixDB (embedded) | ✅ read-only (Milestone 17) | desktop, server |
| LadybugDB | ✅ read-only | **Linux only** (P-144) |
| Postgres/Supabase, Redis/Dragonfly, TypeDB, FalkorDB, HelixDB (HTTP) | 📝 | stub files only |
| Git-host repositories, REST/GraphQL APIs, Julia depot | 📝 | [[Projects and Sources]], [[Julia and Lenticulum]] |
| Writes to any database | 📝 | announced after Milestone 17 (R-41) |

## Platforms

| platform | state |
|---|---|
| Linux desktop (Arch, NixOS), web, server | ✅ tested here |
| Android (Galaxy S10e) | ✅ release APK, signed; no drivers, no PTY, no wasm runtime on the phone |
| macOS | ◐ CI builds; a friend tests source builds (spec [[027]] unverified) |
| Windows | ◐ CI builds; untested |
| iOS | 📝 |

## Most important open problems
1. **CI breadth** (R-39, WP-26) — default browser smoke suites plus the opt-in structural matrix; startup has a shared 60-minute budget, at most two inactivity-only attempts and retained diagnostics ([[P-209 CI startup stalls must retain evidence]]). Several suites remain local-only; three consecutive green runs are still the WP-26 acceptance target.
2. **Security findings** — [[Audit 2026-09-23]], [[Security]]. #1, #4, #8 fixed in their structural part on `refactor` (Milestone 18 phase 4.5); the others are open. Don't expose a server beyond a trusted network.
3. **The core is a library** on `refactor`: `moonkale-core`, `moonkale-ext-api` (documented, `CHANGELOG.md`) and `moonkale-graph-render` (a Rust API, no Moonkale deps) tagged `lib-v1`; the shell depends on no extension; no layering exceptions. `Workspace` is still one large facade — R-37, [[Milestone 18 - Library Refactor]].
4. **Internal state**: SQLite chosen ([[ADR-0014 One store for internal state]]); layouts, user settings, the entity log and agent sessions are in the store (phase 5); nothing syncs yet — R-36, [[Internal State]].
5. Release-build observability (P-032): no log file or panic dialog in installed desktop builds ([[Debugging and Logging]]).

## Next
[[Milestone 18 - Library Refactor]] is merged ([[Milestone 18 - Implementation Log]]; library crates tagged `lib-v1`, Lux.jl in `MathStruct/moonkale-julia`). Spec [[030]] — English/German/Chinese and themes ([[Languages and Themes]]) — is done on the `spec-030` branch; German and Chinese await a native review. The order beyond that is in the [[Roadmap]].

## Sophia history replay (checked 2026-10-24)

ADR-0029 gate 4 now uses reproducible Git-history replay in place of unavailable
daily use of the optional folder store. The opt-in tool and hermetic five-commit
CI test are implemented. Real Moonkale and Lenticulum histories expose missing
and stale live generic names/memberships despite matching file manifests
([[P-231 Sophia folder replay loses generic memberships]]). Sophia fixed P-231
in v0.1.1, which Moonkale pins; with it both replays are clean (0 differences),
so gate 4 is met. Search
stays on the in-memory index (Sophia has no `search` dialect). Gate 3 is met through the server; ADR-0030 switches the legacy graph default. Exact runs and measurements are in [[Indexing]];
this checks the replay only, not the older whole-product inventory above.


## Sophia capped read profile (checked 2026-10-24)

The optional folder store's slow All(1000) read is traced to Sophia v0.1.1:
namespace dependency discovery repeats child enumeration and full metadata
scans. Moonkale's node conversion and generic-call expansion are negligible
by comparison. Sophia fixed it in v0.1.2 (Moonkale pins v0.1.4): All(1000) now takes 43 ms
debug / 16 ms release, and folder stores collect their own garbage (Sophia
SR-146) ([[P-232 Sophia capped graph reads scan namespace metadata]]).
See [[Indexing]] for measurements. ADR-0029's four default gates are met (2026-10-26;
gate 3 by the server fallback, checked in CI). The in-memory index stays the
default until Daniel decides.

## Graph selection compatibility (P-233, checked 2026-10-28)

Experimental structural edits and rename resolve Sophia cache selections to
unique index edit targets while retaining the default Sophia graph. Resolution
uses a native key or exact file/span/input provenance; overloaded or shared
bodies are not chosen automatically. Both editor browser suites exercise real
Sophia ids. See [[P-233 Structural edits need index nodes in the graph]] for the
consumer audit and validation results.
