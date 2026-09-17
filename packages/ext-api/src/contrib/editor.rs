//! `EditorContribution` — "I can open nodes of these kinds".
//!
//! An editor is a panel factory keyed by `NodeKind` + optional `LanguageId`
//! + optional mime, with a `priority` so that, e.g., the WYSIWYG markdown
//! editor beats the plain code editor for `*.md` but the user can still pick
//! "Open with…". This is how the same `.jl` file can open in the code editor
//! or in the Lux.jl drag-and-drop model builder.
