---
title: "lsp-local — implementation notes"
tags: [crate-notes, milestone-3]
---
Notes for `moonkale-lsp-local` (Milestone 3). Native only.

- `StdioTransport::spawn(program, args, cwd)` — starts the server with piped stdio; a writer thread frames outgoing messages with `Content-Length`, a reader thread parses headers and pushes bodies into the incoming channel. Dropping the transport kills the process.
- `discover::find(language) → Option<ServerSpec{program,args}>` — looks the server up on `PATH`, with `rustup which rust-analyzer` as the Rust fallback (rustup's proxy shim would otherwise fail without a toolchain component). Known: `rust → rust-analyzer`, `typescript/javascript → typescript-language-server --stdio`, `python → pyright-langserver --stdio`. `install_hint(language)` gives the one-liner shown in the status bar when nothing is found (`rustup component add rust-analyzer`).

Used by the desktop app in-process and by the server behind `api::lsp_socket`.

Tests: `cargo test -p moonkale-lsp-local` (real `rust-analyzer` on a temp crate: initialize, diagnostics for a type error; skipped if the binary is absent).
