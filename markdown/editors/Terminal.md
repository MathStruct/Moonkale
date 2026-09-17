---
tags: [editor, terminal]
---
# Terminal

Crates: `terminal` (session, VT grid, links), `terminal-pty` (desktop/server PTY), `editors/terminal` (view). Design in [[LSP and Terminal]].

- **View backend**: xterm.js first via [[JS Interop Boundary]] (`packages/js/xterm`, with the WebGL addon); a Rust renderer over `alacritty_terminal`'s grid later — the grid already exists for links/search, so the native path is "just drawing" and can share the [[Graph View]]'s text renderer.
- **Sessions are first-class**: shown in multiple panels, survive panel moves (state lives outside the panel, per `dioxus-workbench` guidance), listed in the palette.
- **Links → graph**: `path:line:col`, URLs, rustc/Julia/Go errors → click opens the node; "open trace as graph" hands a stack trace to [[Indexing]].
- **Commands**: "new terminal here" on any `Directory` node; "run selection" from the [[Code Editor]].
- **Remote**: web/mobile get a PTY inside `api` over a websocket — gated on the security list in [[Platform Matrix]].
