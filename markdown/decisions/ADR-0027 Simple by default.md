---
title: "ADR-0027 — Simple by default (GNOME-like), detailed on request, shortcuts in plain sight"
tags: [adr, ui, ux, keybindings]
status: accepted
date: 2026-10-14
---
**Status:** accepted 2026-10-14. Daniel stated the preference; Claude wrote it up. Related: [[Input Layouts]], [[Overview]], [[Core and Extensions]] (the "on by default" question), [[Editor Prior Art (Obsidian, VS Code, Zed)]].

## Context

**Daniel:** he is unsure whether the UI should be simple, GNOME-like, or complex with more menus, KDE-like. He prefers GNOME's simplicity, but other people have other preferences. He would rather it didn't feel like managing a spaceship, as VS Code does. And shortcuts should be presented prominently, or people won't use them; if they're behind too many clicks in a menu, people won't go there.

Moonkale already grows chrome quickly: a menu bar, an activity rail with about ten entries, panel toolbars, a status bar, tabs. Every extension can add to these. Without a rule, each new extension adds a button, and the spaceship arrives one reasonable button at a time.

## Decision

1. **Two densities, Simple by default.**
   - **Simple** (default, GNOME-like): a few clear controls; one primary action visible per panel; the rest in that panel's **⋯ menu**; search as the main way in (below).
   - **Detailed** (KDE-like, in Settings → Appearance): the full menu bar, more toolbar buttons, a fuller status bar, more visible options.

   It's two levels, not a slider: each is designed and tested. A keymap preset ([[Input Layouts]]) is chosen independently.
2. **A chrome budget, enforced in review:**
   - **activity rail:** at most 7 entries in Simple (the rest under "More");
   - **a panel's toolbar:** at most 4 visible controls in Simple;
   - **status bar:** at most 5 items in Simple, each one meaningful at a glance;
   - **menus:** at most 2 levels deep.

   **Extensions don't add chrome by default:** an extension's rail entry, status item or toolbar button is offered, and the user (or the Detailed density) turns it on. Its commands are always in the palette.
3. **Search first.**
   - The title bar holds **one search field** ("Search or run a command…") showing its shortcut. It opens quick open, the palette and the overview ([[Overview]]) as one place.
   - Every command is reachable **within two clicks or from the palette**; anything deeper is a bug.
4. **Shortcuts in plain sight:**
   - **Everywhere a command appears,** its shortcut is shown: menu items, tooltips (always "Name (Ctrl+…)"), palette rows (already the case), context menus, and the overview's actions.
   - **The empty workbench** (no tab open in a tile) shows the six most useful shortcuts for that moment, Zed- and VS Code-style: open, search, overview, palette, new note, open with context. It updates with the context and the preset.
   - **Hold Ctrl** (about 600 ms, nothing else pressed) and every visible button shows its shortcut as a small badge (key tips, as in Office or Vimium). Releasing hides them.
   - **Gentle nudges:** the third time in a day someone runs a command with the mouse that has a shortcut, a small, non-blocking hint appears once for that command ("Ctrl+Alt+Space does this"). One setting turns them off, and they never interrupt typing.
   - **The cheat sheet** (F1, or hold Ctrl longer) lists the focused panel's bindings, mouse buttons included ([[Input Layouts]] §5).
   - **Getting Started** has one page of the ten shortcuts that matter, per preset, printable.
5. **Defaults over options:** a setting is added only when a reasonable default can't be found. Settings are searchable and grouped, so a new option doesn't make the Settings panel the new spaceship.

## Consequences

- **The current UI gets a pass against the budget:** the menu bar folds into one menu button in Simple, the rail is trimmed to seven with "More", and toolbars are checked panel by panel.
- **Extension guidance changes:** [[Writing an Extension]] says how to contribute a command (always), and that chrome is offered, not imposed.
- **Two densities double some UI tests,** so the browser suites run in Simple; one suite (`density.mjs`) checks Detailed shows the extra chrome and both keep every command reachable.
- **Not decided:** whether the phone shell has a density at all (it's already minimal); themes stay separate from density.

Work packages: WP-63 (densities and the chrome budget) and WP-64 (shortcuts in plain sight) in [[Work Packages]].
