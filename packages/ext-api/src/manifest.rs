//! `Manifest` — the static, declarative half of an extension.
//!
//! Milestone 1 keeps it to identity. The full `moonkale.toml` (permissions,
//! activation events, `[[contributes.*]]` tables) is specified in the vault
//! (`extensions/Manifest Reference.md`) and will be parsed into this type.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Reverse-DNS, stable forever (`"dev.moonkale.editor-code"`).
    pub id: &'static str,
    pub name: &'static str,
}
