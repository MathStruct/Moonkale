---
title: "Overview — every open tab at a glance, then switch"
description: A GNOME-Activities-like overview for Moonkale. One shortcut zooms out to every open tab across tiles and windows, each shown as a live-looking preview of its content (a schematic, not a screenshot), grouped by link group and window, with type-to-search that also reaches quick open. Requested 2026-10-14; the plan with decided defaults.
tags: [editors, workbench, shell, spec, planned]
---
**From Daniel (2026-10-14):** GNOME has a feature where one key shows all open windows of the desktop at once, and you switch between them from there. Moonkale can't use the Windows key, but similar behaviour, where you immediately see what each open tab holds (or schemas or symbols standing for it), would be good.

## What it is

**The overview** is a full-window layer above the workbench. It shows a card for **every open tab**, in every tile and every window of the session, and a search field that already has the focus. Choosing a card switches to that tab and closes the overview. It complements quick open (Ctrl+P), which finds files you have *not* opened, and the tab bar, which shows names, not content.

## Opening it (decided)

- **`Ctrl+Alt+Space`** on desktop and web. It is free in Moonkale's keymap and rarely taken by desktop environments or browsers; Ctrl+Tab is not possible in the web build, where the browser owns it. Configurable like every keybinding.
- **A button** in the activity bar (top) and in the phone's More sheet. On the phone the overview is the natural tab switcher.
- **Optional hot corner** (off by default): moving the pointer into the window's top-left corner.
- **Escape**, the same shortcut, or a click on the background closes it without switching.

## The cards

Each card is a **preview drawn by the extension that owns the tab**: a small, cheap schematic of its content, not a screenshot (screenshots of webview panels are expensive and stale). Examples:

| Tab | Preview |
|---|---|
| Markdown note | its title, the first lines rendered (headings, a formula, a list), its tags |
| Code file | file name, language, the outline of its symbols as a minimap-like column, the cursor's line marked |
| Graph view | a miniature drawing of its current layout (dots and lines from the engine's positions, in the nodes' colours), node and edge counts |
| Flow | the blocks as their symbols at thumbnail size, the issue count |
| Table | its columns and the first rows, the row count |
| Image | the image itself, scaled |
| Terminal | its last lines |
| Conversation | the last exchange, and a dot when the agent is working or waiting for approval |
| Plot, run | the plot at thumbnail size; a run's state and progress |

**On each card:**
- the tab's title;
- its unsaved dot;
- its git status letter;
- its **link group's colour and marker** ([[Linked Panels]]);
- who else is looking at it (presence);
- the source's colour.

A tab whose extension has no preview yet shows its icon, title and path.

## Layout and finding

- **Grouped:** by **link group** first (a note and its local graph side by side as one cluster), then the ungrouped tabs; each window of the session is a labelled section. The order inside each is most recently used first.
- **Typing filters at once:** by title, path, kind and the preview's text. A lone match is preselected, so Enter switches.
- **Below the open tabs**, the same query runs through **quick open** (files, symbols, nodes) and the **command palette**, so one shortcut and a few letters reach anything. This is what GNOME's overview does with apps and files.
- **Keyboard:** arrows move between cards, Enter switches, Shift+Enter opens the card in the tile beside, Delete closes the tab (with the usual unsaved-changes prompt), and Tab moves to the next group.
- **Mouse:** click switches, middle-click closes, and dragging a card onto a tile's edge in the overview docks it there.

## Performance and behaviour (decided)

- **It opens in under 100 ms, with 50 tabs open.** Previews are rendered lazily at card size, cached per document revision, and only for cards on screen (virtualised like the table grid). Measured as a benchmark (BM-01's family, [[Benchmarks]]).
- **It is a snapshot.** Previews don't update live while the overview is open, except the conversation's working dot and a run's progress, which are cheap.
- **Animation:** a 150 ms zoom-out from the current layout to the cards, off when the system or the user asks for reduced motion.
- **Other windows:** on the desktop, choosing a card from another window focuses that window. On the web, a card from another browser tab says "open in another tab" (a page can't focus another tab) and offers to move the tab here.
- **Accessibility:** the overview is a grid of named items, each card's accessible name is its title plus kind plus state, and it's fully usable from the keyboard.

## Extension API

`Extension::preview(panel_id, ws, size) -> Option<Element>` is a new, optional method that draws a small, static preview. Its rules:
- no subscriptions, no I/O beyond what the panel already holds;
- a time budget, beyond which the card falls back to icon and title.

The graph engine gains a `snapshot(width, height)` that draws its current positions into a small canvas, so graph and flow previews don't need a second view.

## Steps

1. The overview layer with icon-and-title cards, grouping by window and link group, typing to filter, the keyboard, and the shortcut. It is useful at once.
2. `Extension::preview` and previews for Markdown, code, table, image, terminal and conversation.
3. The engine's `snapshot`, giving graph and flow previews.
4. Quick open and the command palette below the open tabs.
5. Other windows, drag-to-dock, the hot corner, the animation.

Acceptance (`overview.mjs`):
- with twelve tabs open in two tiles, Ctrl+Alt+Space shows twelve cards in under 100 ms;
- typing "alp" leaves Alpha.md alone and Enter switches to it;
- a linked note and its graph appear side by side with their group's marker;
- Delete on a card closes that tab.

Work package: WP-60 in [[Work Packages]].
