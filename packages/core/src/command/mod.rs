//! Commands and the command bus.
//!
//! Everything a user (or an LLM, or a keybinding, or another extension) can
//! *do* is a `Command`: an id, a human title, an argument schema and a handler.
//! The command palette, menus, keybindings and the LLM tool surface are all
//! generated from the command registry — one registry, four front doors.
//!
//! ```ignore
//! pub struct CommandDescriptor {
//!     pub id: CommandId,           // "editor-code.format-document"
//!     pub title: String,
//!     pub args: ArgSchema,         // JSON-schema-ish; drives palette prompts
//!                                  // and LLM tool definitions
//!     pub when: Option<WhenClause>,// "editorLangId == rust" style predicates
//! }
//! ```
//!
//! `core` only defines the descriptor and the `WhenClause` mini-language.
//! Registration and dispatch live in `moonkale-ext-api` (what extensions
//! see) and `ui` (the shell that owns the palette).

pub mod when;
