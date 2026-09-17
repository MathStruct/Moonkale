//! Built-in `LanguageContribution`s. Each module is data only: file
//! extensions, the tree-sitter grammar asset, highlight queries, comment
//! tokens, and the LSP launch spec. Being here (rather than in `ext-api`)
//! proves that languages need nothing the extension API doesn't provide.

pub mod go;
pub mod julia;
pub mod lean;
pub mod rust;
pub mod unison;
