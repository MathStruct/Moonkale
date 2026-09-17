//! `Manifest` — the static, declarative half of an extension.
//!
//! Lives in `moonkale.toml` next to the extension's `Cargo.toml`, parsed
//! before any code runs, so the shell can build menus, keybindings and the
//! command palette for an extension that has not been activated yet (lazy
//! activation is how we keep startup fast with hundreds of extensions).
//!
//! ```toml
//! [extension]
//! id          = "dev.example.unison"
//! name        = "Unison Language"
//! version     = "0.1.0"
//! api         = "^0.1"                 # moonkale-ext-api semver requirement
//! kind        = "wasm"                 # or "static"
//! platforms   = ["desktop", "web"]     # omit => all
//!
//! [permissions]
//! sources     = ["read"]               # read | write
//! network     = ["https://share.unison-lang.org"]
//! process     = false                  # spawn subprocesses (desktop only)
//! fs          = []                     # extra paths outside the workspace
//!
//! [activation]
//! on = ["language:unison", "command:unison.*", "view:unison.codebase"]
//!
//! [[contributes.language]]
//! id = "unison"; extensions = [".u"]; tree_sitter = "grammars/unison.wasm"
//!
//! [[contributes.panel]]
//! id = "unison.codebase"; title = "Codebase"; home = "side"
//! ```
//!
//! Every `[[contributes.*]]` table maps 1:1 to a type in `contrib::*`.
