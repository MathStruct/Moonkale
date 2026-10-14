---
title: "Input layouts — the five-button mouse, keymap presets, and keyboards other than US"
description: Developers using Moonkale are assumed to have at least a five-button mouse, for full functionality (Daniel, 2026-10-14; spec 025 becomes a requirement). The default mouse map, an extended map for gamer mice with more buttons (through F13–F24), keymap presets to choose from (Moonkale, VS Code, JetBrains, Vim, Emacs, Obsidian, and a left-hand layout for mouse-heavy graph work), and correct shortcuts on QWERTZ, AZERTY, Dvorak and other layouts.
tags: [platform, input, keybindings, mouse, spec, planned]
---
**From Daniel (2026-10-14):** assume developers in Moonkale have at least a five-key gamer mouse for full functionality, and suggest multiple key layouts. Spec [[025]] (documented 2026-09-22) described the five-button mouse; this page makes it a requirement and adds the layouts.

**Two-button users keep everything reachable:** every mouse action has a keyboard shortcut and a menu entry. Five buttons make Moonkale *faster*, never *possible*.

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

**The webview's own handling** of buttons 4 and 5 is history navigation of the *page*, which would navigate out of the app. It is prevented on desktop (WebKitGTK), web and Android ([[025]]'s warning, verified per platform in `mouse.mjs`).

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
- **Tested layouts:** US, German (QWERTZ, Daniel's), French (AZERTY), Dvorak, and a Chinese IME session.
- **The IME's own keys win:** Ctrl+Space and Shift switch input methods in fcitx and IBus. A binding on them is flagged as a conflict when an IME is active (completion falls back to Ctrl+. there).

## 4. Conflicts the presets must avoid

- **Desktop environments:**
  - GNOME uses **Super** for its overview, which is why Moonkale's is Ctrl+Alt+Space;
  - GNOME and KDE switch workspaces with **Ctrl+Alt+arrows**, so presets don't bind those;
  - KDE uses Ctrl+F1–F4.
- **Browsers (the web build):** Ctrl+W, Ctrl+T, Ctrl+N, Ctrl+Tab and Ctrl+Shift+T can't be taken from the browser. Presets name an alternative for each in the web build, and the table shows which bindings the browser keeps.
- **Text fields:** a binding never steals a character a text field needs (the rule that put AltGr out of bindings).

## 5. Seeing what's bound

- **A cheat sheet:** hold Ctrl for a second, or press F1. It's an overlay with the active preset's bindings for the focused panel, mouse buttons and F13–F24 included, and it can be printed per preset.
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
