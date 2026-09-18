//! `Manifest` — the static, declarative half of an extension.
//!
//! Milestone 6 adds what the *Extensions* settings need: a description,
//! whether the extension can be switched off and whether it starts enabled,
//! and the permissions it asks for (declared here, granted in Settings,
//! checked by the host on every call — [[Extension System]]). The full
//! `moonkale.toml` for third-party extensions is parsed into this type by
//! `ext-host`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Reverse-DNS, stable forever (`"dev.moonkale.editor-code"`).
    pub id: &'static str,
    pub name: &'static str,
    /// One line for the Extensions list.
    pub description: &'static str,
    /// `false` for the core (explorer, settings): no toggle is shown.
    pub optional: bool,
    /// Starting state for optional extensions (the user may override).
    pub default_enabled: bool,
    /// What the extension needs: `"read-sources"`, `"write-files"`,
    /// `"run-commands"`, `"network"`. Empty for pure UI.
    pub permissions: &'static [&'static str],
}

impl Manifest {
    /// A core extension: always on, no toggle.
    pub const fn core(id: &'static str, name: &'static str, description: &'static str) -> Self {
        Self {
            id,
            name,
            description,
            optional: false,
            default_enabled: true,
            permissions: &[],
        }
    }

    /// An optional extension that starts enabled.
    pub const fn optional(id: &'static str, name: &'static str, description: &'static str) -> Self {
        Self {
            id,
            name,
            description,
            optional: true,
            default_enabled: true,
            permissions: &[],
        }
    }

    /// An optional extension that starts **disabled** (Daniel's rule for
    /// heavyweight or niche features such as Lux.jl).
    pub const fn opt_in(id: &'static str, name: &'static str, description: &'static str) -> Self {
        Self {
            id,
            name,
            description,
            optional: true,
            default_enabled: false,
            permissions: &[],
        }
    }

    pub const fn with_permissions(mut self, permissions: &'static [&'static str]) -> Self {
        self.permissions = permissions;
        self
    }
}
