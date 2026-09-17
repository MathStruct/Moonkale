//! # moonkale-editor-markdown
//!
//! Knowledge editing. **Milestone 2** ships the part that needs the index:
//! a *Links* panel showing the active document's backlinks (pages that link
//! here), outgoing links, and unresolved targets — all read from the index
//! source, so it works for any file the extractors understand.
//!
//! Still design notes (vault `editors/Markdown and Typst Editor.md`):
//! WYSIWYG via a `RichTextBackend` (Milkdown), the Rust-side block model,
//! link completion and rename propagation, Typst preview.

pub mod extension;
pub mod links_panel;

pub use extension::LinksExtension;
pub use links_panel::LinksPanel;
