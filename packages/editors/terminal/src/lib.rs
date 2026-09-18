//! # moonkale-editor-terminal
//!
//! One workbench panel ("Terminal", bottom tile) holding any number of
//! sessions as its own tabs. Each session is a `moonkale_terminal::Session`
//! whose backend the platform provides (local PTY on desktop, websocket to
//! the server on web); the view is xterm.js behind the interop boundary
//! (`packages/js/xterm`, `assets/xterm.js`).
//!
//! Sessions live in the extension (signals at `ScopeId::ROOT`), so docking
//! the panel elsewhere keeps them running. Ctrl+click on a `path:line` in
//! the output opens the file.

pub mod extension;
pub mod panel;

pub use extension::TerminalExtension;
