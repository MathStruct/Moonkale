//! `LanguageContribution` — a programming or markup language.
//!
//! `id`, `aliases`, file `extensions`, `first_line` regex, a tree-sitter
//! grammar (as a `.wasm` asset — the same artefact works on web and native),
//! highlight/injection/locals queries, comment tokens, bracket pairs, and an
//! optional `lsp` block (`command`, `args`, `transport`) that `moonkale-lsp`
//! uses to start or connect to a language server.
//!
//! The initial set (Rust, Julia, Go, Unison, Lean) are static extensions in
//! `packages/editors/code/src/languages/` so we dogfood the contribution point
//! from day one.
