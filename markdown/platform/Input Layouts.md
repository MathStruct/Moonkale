---
title: "Input layouts — the five-button mouse, keymap presets, and keyboards other than US"
description: Developers using Moonkale are assumed to have at least a five-button mouse, for full functionality (Daniel, 2026-10-14; spec 025 becomes a requirement). The default mouse map, an extended map for gamer mice with more buttons (through F13–F24), keymap presets to choose from (Moonkale, VS Code, JetBrains, Vim, Emacs, Obsidian, and a left-hand layout for mouse-heavy graph work), and correct shortcuts on QWERTZ, AZERTY, Dvorak and other layouts.
tags: [platform, input, keybindings, mouse, spec]
---
**From Daniel (2026-10-14):** assume developers in Moonkale have at least a five-key gamer mouse for full functionality, and suggest multiple key layouts. Spec [[025]] (documented 2026-09-22) described the five-button mouse; this page makes it a requirement and adds the layouts.

**Two-button users keep everything reachable:** every mouse action has a keyboard shortcut and a menu entry. Five buttons make Moonkale *faster*, never *possible*.

## Built scope (WP-61, 2026-10-28)

The shipped presets are **Moonkale, VS Code, Obsidian and Left hand + mouse**.
Settings → Keybindings has one chooser; explicit command overrides and empty
unbinds remain above the preset, across changes. Presets are bundled JSON base
layers in `packages/shell/keymaps`; extension defaults fill commands a preset
does not change. Duplicate effective bindings are marked in the editor;
explicit overrides resolve before base bindings. The command registry feeds
menus, tooltips, key tips and the cheat sheet from the same effective bindings.
Obsidian includes Ctrl+O for Go to File, Ctrl+P for the palette, Ctrl+E for
Source/Rich Markdown and Ctrl+Shift+F for search.

The `mouse` settings object maps `back`, `forward` and `middle` to command ids.
The defaults are `navigation.back`, `navigation.forward` and `contextual`.
Empty strings disable a button; left/right selection and context gestures remain
owned by each surface. Thumb events are captured before the browser/webview's
page-history handlers. Visits are window-local, capped at 256 and shared by
foreground documents and committed graph-camera gestures. They are separate
from edit/undo history and are not persisted or shared across windows. Link-group
subjects remain WP-59.

Middle-click loads Explorer files, quick-open results, Links-panel files, graph
files/symbol owners and table references in background tabs. It closes a tab
and pans either canvas, including when the drag starts over a node. Go to File's
**Alt+Enter** and the Explorer context menu offer background opening without a
middle button. Inline links inside rich/text editors retain their existing
editor-owned behaviour. Table rows without a node/reference have no document
to open. Background attachment preserves each tile's selected tab.

F13–F24 are bindable. Shipped extra defaults cover existing commands: F14
palette, F16 graph Local/Whole, F17 definition, F18 references, F19 active table
query/flow generation and F22 split right. F13 (overview), F15 (link-group
context), F20 (proposal approval), F21 (comments) await their owning packages;
F23/F24 are unbound. Settings includes a dedicated key/button test field and
remapper setup guidance; Tab/Shift+Tab leave the test field normally. It does
not record typing in documents or secret fields.

Punctuation bindings match US physical codes; letters match logical keys.
Chromium's layout map supplies labels when permitted, with US labels and an
explicit Settings hint as the fallback. AltGraph and composition are excluded;
ordinary registered Ctrl+Alt chords still dispatch. Native OS layout-map wiring,
real OS keyboard/IME runs, and desktop WebKit/Android mouse verification remain
platform acceptance work. Browser cases simulate German/French/Dvorak key/code
pairs and composition; they do not switch the OS layout or prove a live IME.

The left-hand preset enables WASD pan, Q/E zoom, F fit, held Space+drag and
Tab along graph edges on focused canvases; flow keeps R/M rotate/mirror. Graph
also has G for Local/Whole. Tables use WASD cell movement, Q/E page movement,
F filter and Tab/Shift+Tab cells. Text inputs retain their letters. Link-group
1–8, Vim, Emacs and JetBrains remain planned. Existing canvas wheel zoom is
unchanged; editor font zoom, wheel tilt and per-surface wheel remapping remain
future work. Canvas context menus and some inline symbol/link actions are
still owned by their respective editor work packages.

`mouse.mjs` and `keymaps.mjs` are registered in run-all, its README and CI smoke.
See [[Status]] and the handoff for validation; the tables below retain the broader
target, including actions whose owning feature does not exist yet.

## 1. The mouse

### Five buttons: the default map

| Button | Default | Where |
|---|---|---|
| Left | select, drag, open | everywhere |
| Right | context menu (it lists the double-click actions of spec [[024]], so nothing hides behind a double-click) | everywhere |
| **Middle** (wheel click) | **open beside** (a new tab without switching focus) on a file, symbol, link, graph node or table row; **close** on a tab; **pan** while held on a canvas (graph, flow) | everywhere |
| **Back** (thumb, button 4) | **back** through the session's navigation history: documents visited, the graph camera, a link group's subject ([[Linked Panels]]) | everywhere |
| **Forward** (thumb, button 5) | **forward** through the same history | everywhere |
| Wheel | scroll; with **Ctrl** zoom (graph, flow, editor font); with **Shift** scroll sideways | everywhere |
| Wheel tilt (left/right) | scroll sideways; on the tab bar, the previous or next tab | where the mouse has it |

**The webview's own handling** of buttons 4 and 5 is page-history navigation, which would navigate out of the app. A shared capture guard prevents the default; `mouse.mjs` exercises Chromium. Desktop WebKitGTK and Android verification remains manual.

### More buttons: the extended map

Browsers and webviews receive only the five standard buttons. Extra buttons on gamer mice (6 to 12, the DPI-shift button, side panels) reach Moonkale only as **keys**, mapped by the mouse's own software. So Moonkale binds the **F13–F24** keys, which no keyboard has, by default:

| Key | Default command |
|---|---|
| F13 | Overview ([[Overview]]) |
| F14 | Command palette |
| F15 | Open with context ([[Linked Panels]]) |
| F16 | Graph: Local ↔ Whole |
| F17 | Go to definition / centre the graph on the symbol (spec [[024]]) |
| F18 | Find references |
| F19 | Run / Generate (the active document's main action: run a query, generate a flow's code, start a run) |
| F20 | Approve the selected agent proposal ([[Shared Work]]; it still shows the diff first) |
| F21 | Toggle comments panel |
| F22 | Split the editor |
| F23, F24 | unbound, for the user |

**The setup guide** (Settings → Input → Mouse, and a page in Getting Started) says how to map extra buttons to F13–F24:
- on **Linux**: Piper/libratbag for supported mice, input-remapper for any;
- on **Windows and macOS**: the vendor's software (Logitech Options+, Razer Synapse, Steelseries GG) or a general remapper.

It also explains how to check a mapping (Settings → Input shows the last key or button it received).

## 2. Keymap presets

A **preset** is a whole keymap shipped as a data file. The user picks one in Settings → Keybindings, sees its table before choosing, and their own overrides stay on top of whichever preset is active. Conflicts are listed, never silent.

| Preset | For whom | Notes |
|---|---|---|
| **Moonkale** (default) | everyone | today's bindings, plus the mouse maps above |
| **VS Code** | most developers | its well-known bindings for the commands Moonkale shares (Ctrl+P, Ctrl+Shift+P, F12, Shift+F12, Ctrl+/, Ctrl+B, Ctrl+J, Ctrl+`, Alt+Z) |
| **JetBrains** | IntelliJ, CLion, RustRover users | Double-Shift for search everywhere, Ctrl+B for definition, Alt+Enter for actions |
| **Obsidian** | note-takers | Ctrl+O quick switcher, Ctrl+E reading/editing, Ctrl+Shift+F search |
| **Vim** | modal editing | a modal layer for the code editor (normal, insert, visual) as a keymap extension; the rest of the app keeps the base preset |
| **Emacs** | Emacs users | chords (C-x C-f) and text movement; on macOS the system text keys stay as they are (spec [[027]]) |
| **Left hand + mouse** | graph, flow and table work with the right hand on the mouse | commands on the keys under the left hand (below) |

**macOS** needs no separate preset: `Ctrl` in a binding means Cmd on a Mac (spec [[027]]), so every preset works there.

### The left-hand layout

For long sessions on the graph, the flow editor or a table, the left hand stays on the keyboard and the right on the mouse, as in games and in CAD.

| Key | On a canvas (graph, flow) | In a table |
|---|---|---|
| W / A / S / D | pan up, left, down, right | move the cell cursor |
| Q / E | zoom out / in | previous / next page |
| R / M | rotate / mirror the selected block (ADR-0024) | — |
| F | fit the view | filter box |
| G | Local ↔ Whole | — |
| Space (held) | pan with the mouse | — |
| Tab | next node along edges (spec 031's keyboard navigation) | next cell |
| 1–8 | jump to link group 1–8 | — |
| Shift / Ctrl + click | add to selection / box select | extend selection |

These keys act only while a canvas or table has the focus. A text field always gets its letters.

## 3. Keyboards other than US

- **Letters follow the letter.** `Ctrl+Z` means the key that types *z*, wherever it is (on QWERTZ it's where US has *y*). Moonkale matches the logical key today (`KeyboardEvent.key`), which is right for letters.
- **Punctuation follows the position.** `Ctrl+/`, `Ctrl+[`, `Ctrl+\`, `Ctrl+`` ` and similar bindings are defined by the US *position* of the key (`KeyboardEvent.code`). Otherwise they are unreachable or need AltGr on QWERTZ and AZERTY: on a German keyboard `/` is Shift+7, and `[` is AltGr+8.
- **Labels show what's printed on the user's key:** a binding defined as the US `Slash` position shows as `Ctrl+-` on a German keyboard. Chromium's `navigator.keyboard.getLayoutMap()` gives the labels on the web. On the desktop the layout comes from the OS; until that's wired, the label falls back to the US name with a hint.
- **AltGr is never a modifier for bindings.** It types characters (`@`, `{`, `|` on QWERTZ), and a binding on it would swallow them.
- **Browser-covered layouts:** US plus simulated German/French/Dvorak physical-key events; composition exclusion is covered. Real OS layout and Chinese IME sessions remain manual.
- **The IME's own keys win:** Ctrl+Space and Shift switch input methods in fcitx and IBus. An IME-aware conflict indicator and automatic completion fallback remain planned; these are not supplied by WP-61.

## 4. Conflicts the presets must avoid

- **Desktop environments:**
  - GNOME uses **Super** for its overview, which is why Moonkale's is Ctrl+Alt+Space;
  - GNOME and KDE switch workspaces with **Ctrl+Alt+arrows**, so presets don't bind those;
  - KDE uses Ctrl+F1–F4.
- **Browsers (the web build):** Ctrl+W, Ctrl+T, Ctrl+N, Ctrl+Tab and Ctrl+Shift+T can't be taken from the browser. Presets name an alternative for each in the web build, and the table shows which bindings the browser keeps.
- **Text fields:** a binding never steals a character a text field needs (the rule that put AltGr out of bindings).

## 5. Seeing what's bound

[[ADR-0027 Simple by default]] §4 puts shortcuts in plain sight: in every tooltip and menu, on the empty workbench, as key tips while Ctrl is held, and as once-per-command nudges. Then:

- **A cheat sheet:** press F1 (holding Ctrl currently shows key tips). It's an overlay with the active preset's bindings for the focused panel, mouse buttons and F13–F24 included, and it can be printed per preset.
- **Settings → Input:** the preset, the overrides, the mouse maps, the last key or button received, and the conflicts.

## Steps

1. The five-button map (spec 025's items, with the webview guard) and the cheat sheet.
2. Presets as data files, Settings → Keybindings with preset choice and conflicts; the VS Code and Obsidian presets first.
3. Positional punctuation bindings and layout labels; QWERTZ, AZERTY and Dvorak in tests.
4. F13–F24 defaults and the setup guide.
5. The left-hand layout on graph, flow and table.
6. JetBrains and Emacs presets; Vim as a keymap extension.

Acceptance (`mouse.mjs`, `keymaps.mjs`):
- the thumb buttons go back and forward through two documents and a graph camera move without leaving the app;
- middle-click on a tree file opens it beside;
- with a German layout, the comment toggle's binding (US `Ctrl+/`) works on `Ctrl+-` and is labelled so;
- switching to the VS Code preset makes `Ctrl+Shift+P` the palette and keeps the user's overrides.

Work package: WP-61 in [[Work Packages]].
