//! # moonkale-editor-markdown
//!
//! Knowledge editing. Two formats, one panel:
//!
//! - **Markdown** with Obsidian conventions: `[[wiki-links]]`, `[[page#heading]]`,
//!   `![[embeds]]`, frontmatter, tags, callouts. WYSIWYG via a Milkdown
//!   (ProseMirror) backend behind `trait RichTextBackend`, same pattern as
//!   the code editor. Source-mode fallback is the code editor.
//! - **Typst** — compiled by the `typst` crate *in process* (it is pure Rust
//!   and compiles to wasm), rendered to SVG pages in a split preview. No
//!   WYSIWYG for Typst initially; the code editor with live preview is the
//!   plan, and a structured editor is a later research item.
//!
//! Links are the bridge to the graph: every `[[link]]` is a `Links` edge
//! (extracted by `moonkale-index`), backlinks are a query, and the "local
//! graph" side panel is just the graph editor showing a depth-1 view.
//!
//! See vault: `editors/Markdown and Typst Editor.md`.

pub mod backend;
pub mod links;
pub mod model;
pub mod panel;
pub mod typst;
