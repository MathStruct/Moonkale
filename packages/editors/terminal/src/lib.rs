//! # moonkale-editor-terminal
//!
//! The terminal *view*. Session logic (bytes, grid, links) is in
//! `moonkale-terminal`; this crate draws it and forwards keystrokes.
//! Same swappable-backend pattern as the other editors:
//!
//! - `xterm` backend: `packages/js/xterm` bundle; fastest path to a
//!   good terminal on all platforms.
//! - `native` backend: draw `alacritty_terminal`'s grid with a Rust
//!   renderer (a Dioxus virtualised grid, or the graph crate's wgpu text
//!   path). The grid already exists for links/search, so this is "just"
//!   drawing.
//!
//! Panel features: multiple sessions as tabs, split (via the workbench),
//! clickable links → open node in editor, "run selection" from the code
//! editor, and a "new terminal here" command on directory nodes.

pub mod backend;
pub mod input;
pub mod panel;
