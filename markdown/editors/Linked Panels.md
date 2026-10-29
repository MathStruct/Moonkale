---
title: "Linked panels — a node's content and its context, together and marked as belonging together"
description: Panels that belong together (a node's content and the local graph around it, its backlinks, its rows, its plot) form a link group. They follow one subject, stay in sync, and are visibly marked (a shared tab colour plus a marker, never colour alone). Requested 2026-10-14; the plan, decided defaults, and the implementation steps.
tags: [editors, workbench, graph, spec, implementation]
---
**From Daniel (2026-10-14):** certain panels should be linked, for example the context graph of a node (the graph locally around it) and the node's content. Tabs that belong together share a colour, or are otherwise visibly marked.

**Implemented and validated (WP-59, 2026-10-29):** steps 1–5 are built. The global graph retains active-editor following; named groups own independent local graph views. The Links panel still follows the active document. Validation and later work are recorded in the implementation notes below.

## Concepts

- **A link group** is a set of panels that show one **subject**: a node, or a selection of nodes. One member is the **lead** (usually the content: a document, a table, a flow); the others are **followers** that show the subject's context:
  - its local graph;
  - its backlinks;
  - its outline;
  - its rows;
  - its runs and plots;
  - its comments.
- **How a follower follows:**
  - **group:** it shows whatever its group's lead shows (the default);
  - **pinned:** it stays on one node even when the lead moves on ("keep this neighbourhood while I read elsewhere");
  - **active:** it follows whichever editor is active, which is today's behaviour, kept as the mode of the one ungrouped graph panel.
- **Several groups at once:** two documents, each with its own local graph, side by side. This needs **graph panel instances**, one per group. The engine already supports several views (`window.moonkale.graphViews` is a map), so each instance is its own view with its own camera and stash.

## Behaviour

- **The lead moves, followers follow:**
  - opening another node in the lead (a link, quick open, back and forward) recentres the local graph and refreshes backlinks;
  - the cursor's symbol in a code file (spec [[024]]) is an optional, finer subject.
- **A follower can steer, without surprises:**
  - **click** a node in the group's graph: it's selected and highlighted, and the lead's view of it is revealed if it's the subject itself;
  - **double-click:** the group's lead opens that node (the group navigates, back returns);
  - **Alt+double-click:** it opens in a new group (peek, keep this one).
- **Selections sync** both ways where the lead can show one: a table row selected in the lead is the node selected in the graph, and a block selected in a flow is the node selected in its graph.
- **Closing:**
  - closing the lead closes its group-mode followers;
  - pinned followers stay, become ungrouped, and keep their colour until they're closed;
  - closing a follower leaves the group intact.
- **Moving:** group members can sit in any tile or window. Dragging one elsewhere keeps the link. A group can span windows (the session bus already moves documents between windows).
- **Persistence:** groups, members, roles, pins and colours are part of the session ([[ADR-0025 Sessions are the outermost state]]), so they come back on restart and on switching sessions.

## How a group is marked (decided)

- **A colour per group** from a palette of eight theme tokens (`--mk-link-1` … `--mk-link-8`, defined in the theme for dark and light, so the colour check stays clean). The palette is distinct from the source colours (Projects) and from git's status colours, so a group stripe can't be read as "modified".
- **The tab** of every member gets a 3 px stripe in the group's colour along its top edge, plus a small **marker** (the group's number, ①–⑧, or a letter), because colour alone fails colour-blind users and grey-scale screenshots.
- **Hovering a member's tab** outlines the other members' tabs, in every tile, so "which ones belong together" is one hover away.
- **The panel itself** gets a faint tint of the colour on its header bar. The content stays untinted.
- **The tab's tooltip and its accessible name** say "linked with *X*, *Y* (group 3)", so a screen reader hears it too.
- **Ungrouped panels** have no stripe. The one panel in "active" mode shows a small ↯ marker: "follows the active editor".

## Making and breaking groups

- **"Open with context"** (command palette, the Explorer's context menu, Ctrl+Alt+Enter, and the graph's double-click with a modifier): opens the node's content and its local graph side by side as a new group. A setting chooses which followers come with it (local graph by default; backlinks, outline, comments optional).
- **Link to…** on a tab's context menu: join this panel to a group as a follower, or pin it.
- **Dragging a tab onto another while holding Alt** links them.
- **Unlink** on the context menu ends a panel's membership; the group keeps the others.
- **At most eight groups** get distinct colours; beyond that colours repeat with distinct numbers.

## Beyond the graph

The same mechanism links other pairs, each a lead with followers:
- a **flow** and its generated code (`circuit.jl`, `inference.jl`): select a block, see its lines;
- a **table** and the node page of the selected row;
- a **conversation** and the files its agent touched;
- a **run** and its plots ([[Lenticulum Frontend Requirements]] LT-12: one time cursor across the group);
- a **proposal** and its diff panels ([[Shared Work]] SW-23);
- a **commit** in history and the files it changed.

## Implementation steps

1. **The model:** `ext-api` gains `LinkGroup { id, colour, members: [(panel id, Role, Follow)] }` and `Workspace::link_subject(panel_id) -> Option<Subject>`. Panels that follow read their subject from it instead of from "the active document". The groups are stored in the session.
2. **The shell marks tabs:** through `dioxus-workbench`'s `Panel::with_class` (the stripe and tint come from a `mk-link-N` class), `with_tab_accessory` (the marker), the hover outline, and the accessible name. Theme tokens are added for the eight colours.
3. **Graph panel instances:** the graph extension contributes one panel per group (`graph:<group>`) beside the global one; each owns its view, camera and Local stash.
4. **Open with context,** Link to, Alt-drag, Unlink, close rules.
5. **Selection sync and steering** for the document, table and flow leads.
6. **The other pairs** of "Beyond the graph", as their packages land.

Acceptance (browser suite `linked-panels.mjs`):
- Open with context on `Home.md` gives two tabs with one stripe colour and one marker;
- following a wiki-link in the note recentres the group's graph, while a second group on `Alpha.md` keeps its own graph;
- double-clicking a node in the graph navigates the group, and Back returns;
- closing the note closes its graph;
- the groups survive a reload.

Work package: WP-59 in [[Work Packages]].

## WP-59 implementation notes (2026-10-29)

The ext-api link model owns subjects, members, lead/follower roles, following
modes and bounded group navigation. `SavedWorkspace` record version 2 adds groups
and graph view/camera metadata plus lead-to-document identities; version 1
migrates on read through defaulted
fields. Content remains in its document signal. Each group has a stable lead
panel identity, allowing two groups to view the same document independently.
The shell resolves these identities through the winning editor contribution.

Tab accessories carry numbered markers; eight light/dark theme tokens provide
the stripe, hover outline and faint toolbar tint. Accessible names name the
other members and group number. The original Graph tab carries ↯. Commands
use the effective keymap; the Explorer and tab context menus provide pointer
routes. Shift+F10 opens a focused tab's menu. Alt-drag links tabs without
changing their placement. Unlinking a graph retains its view and subject.

Graph instances use separate engine views, cameras and Whole/Local stashes.
Wiki navigation and graph steering update their originating group. Table
results and flow blocks supply their own context graph and selection identities.
Group Back/Forward retains node and block/row selection independently of edit
history. Closing a lead removes its
group-following graphs; pinned contexts retain their subject and colour.

Automatic side-by-side placements are persisted with the workbench layout;
restoration also reconstructs pairs for records lacking those placements. Editor
DOM hosts are keyed by panel instance. CodeMirror loads its bundled module once
per document and checks mount ownership before applying messages or destroying
a host, so remounts cannot remove a replacement editor.

Validation passes: model, version-1 migration, navigation, pin/unlink, import
bounds and close-rule unit tests; `linked-panels.mjs` (including opening an
existing lead behind Settings, session switch and reload with both editors
visible); all AGENTS gates; and the full smoke, structural, Sophia and
platform-default graph browser matrix. A private desktop screenshot was
inspected with two restored, marked note/graph pairs side by side; persisted
metadata and unchanged source files were checked. The remount fixes also pass
sessions, keyboard/mouse journeys, properties and CodeMirror search regressions.
Link coordination is currently within one workspace window; cross-window group
transport remains later work.

**Step 6, “Beyond the graph”, remains later work:** generated-code pairs,
conversations/files, runs/plots, proposals/diffs and commits/files are not added
by WP-59.
