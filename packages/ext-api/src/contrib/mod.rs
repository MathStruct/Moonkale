//! Contribution points — the *kinds of things* an extension can add.
//!
//! Each module here is one contribution point and corresponds to a
//! `[[contributes.<name>]]` table in the manifest. Adding a contribution point
//! is an API change; adding a contribution is not.

pub mod command;
pub mod editor;
pub mod keybinding;
pub mod language;
pub mod llm_tool;
pub mod panel;
pub mod renderer;
pub mod source;
pub mod theme;
