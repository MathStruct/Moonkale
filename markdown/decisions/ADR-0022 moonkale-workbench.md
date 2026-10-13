---
title: "ADR-0022 — moonkale-workbench: our own panel and tile shell"
tags: [adr, shell, workbench, ui]
status: accepted
date: 2026-10-13
---
**Status:** accepted 2026-10-13 (Daniel: "we don't own the repository, so we gonna do our own moonkale-workbench to fix that problem" — the same call as [[ADR-0018]] for `dioxus-flow`). Supersedes [[ADR-0010 dioxus-workbench for layout]]. Related: [[ADR-0018 One frontend graph engine]], [[Problem Log]] P-181, [[Overview]], [[Project Structure]].

## Context

The shell's tabs, splits and docking come from `dioxus-workbench` 0.1.0 — a third-party crates.io crate (the same author as `dioxus-flow`, which ADR-0018 retired). ADR-0010 adopted it because nothing like Lumino existed for Dioxus, and it works: drag-to-dock, splits, serde layout persistence, `--wb-*` theming, ARIA.

P-181 (2026-10-13) is the crack: open a document from the explorer tree, edit it, and the editor's tab goes to the background — the panel renders at 0×0 with `aria-selected="false"` while its model takes the edit. The cause is structural, not a typo:

- **Two owners of "what is in front".** The shell owns `docs.active` and passes `active_panel`; the crate *also* keeps its own `tile.active` per tile, reconciled through an activation edge that remembers what it last applied and a registry reconcile that resets a tile's active tab to its first panel. Any state where the two disagree — a tree-click open, the first dirty-dot accessory re-render — is a hiding tab.
- **We cannot fix it.** The tab state, the reconcile and the activation guard all live in the crate. A fix means patching a 0.1.x crate we do not own, at our pace, in our CI — exactly the `dioxus-flow` situation (ADR-0018 §Context), and "The bugs in the Problem Log with a Dioxus name on them each cost hours" is already on record ([[Why Not a Plugin or Electron]]).

ADR-0010 also listed known limitations we would wait on: content remounts on structural moves, no floating panels, no maximise, no tab overflow. Waiting on someone else's roadmap is the same dependency.

## Decision

1. **`moonkale-workbench`** — our own crate (`packages/workbench`), owning tabs, tiles, splits, drag-to-dock and the tab strip. The shell stays the owner of *documents*; the workbench is a layout engine, not a state owner.
2. **One source of truth.** The foreground panel is `docs.active`'s panel, resolved by the shell and applied by the workbench — the workbench keeps no "active" that can disagree. A registry change (a dirty dot appearing, a panel closing elsewhere) never moves the foreground tab. This is the P-181 rule, made structural.
3. **Same props, gradual swap.** The shell's `PanelWorkspace` props are the boundary (`panels`, `strings`, `layout`, `reset_layout`, `active_panel`, `on_panel_close`, `on_panel_activate`, `on_layout_change` — plus the phone variant). `moonkale-workbench` grows behind them, gated on a parity checklist like ADR-0018's stage 7: dock/split gestures, serde layout round-trip (settings persistence), `--wb-*` theming, ARIA roles, keyboard (Alt+Shift+Arrows split, Alt+Shift+Page dock), the touch-drag shim, the narrow/phone collapse, closable panels, tab accessories. When the checklist holds, `dioxus-workbench` is retired — dependency, CSS, everything, the `dioxus-flow` pattern.
4. **ADR-0010's limitations become our roadmap, decided by us**: floating panels, maximise, tab overflow, and whether panel content survives structural moves (today state must live outside panels — a rule that stays until we deliberately lift it).

## Consequences

- The shell's layout code has an owner in-repo; the P-181 class of bug becomes a local fix, not an upstream negotiation. The e2e suites' `:visible`-filtering workarounds and the tree-open tab loss are the first things the parity checklist must not reproduce.
- Layout persistence stays serde-compatible (`PanelLayout` in workspace settings) so no user's saved layout breaks on the swap.
- ADR-0010 is superseded; its context ("nothing like Lumino existed") remains the honest record of why the crate was right for the prototype, as [[research/dioxus-flow]] does for the canvas.
- What we do **not** decide here: the crate's internal architecture (its own design pass comes first, the ADR-0018 pattern), and whether `moonkale-workbench` grows non-layout shell surfaces (status bar, rail — the `rail`/`status` slots stay the shell's for now).
