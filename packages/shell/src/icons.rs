//! The source rows' icons and colours (spec 009). The icon set itself
//! moved to `moonkale_ext_api::icons` (2026-10-14) so every extension's
//! toolbar can use it.

pub use moonkale_ext_api::icons::Icon;

/// The icon and a human label for a source (spec 009): by family, refined
/// by the `Custom` name the drivers use (`"git"`, `"data"`, …) and by the id
/// prefix (`ssh:`, `repo:`, `api:`).
pub fn source_icon(d: &moonkale_core::SourceDescriptor) -> (&'static str, &'static str) {
    use moonkale_core::SourceFamily as F;
    let id = d.id.as_str();
    match &d.family {
        F::Folder if id.starts_with("ssh:") || id.starts_with("remote:") => {
            ("folder-remote", "remote folder")
        }
        F::Folder => ("folder", "folder"),
        F::Sql if d.display_name.contains("data files") || id.starts_with("data:") => {
            ("data", "data folder")
        }
        F::Sql => ("database", "database"),
        F::Graph => ("graphdb", "graph database"),
        F::KeyValue => ("kv", "key-value store"),
        F::Index => ("index", "index"),
        F::Remote => ("folder-remote", "remote"),
        F::Custom(name) if name == "git" || name == "repo" => ("repo", "repository"),
        F::Custom(name) if name == "api" => ("api", "API"),
        F::Custom(_) => ("puzzle", "source"),
    }
}

/// A stable accent colour for a source: one of eight palette entries chosen
/// by a hash of the id, so it is the same on every machine (spec 009; the
/// user-chosen colour of a project overrides it later).
pub fn source_color(id: &str) -> &'static str {
    const PALETTE: [&str; 8] = [
        "#4fb3e8", "#e0a84a", "#8fd18f", "#e07a7a", "#b48fe0", "#5fd1c1", "#e08fc7", "#c9c95a",
    ];
    let mut h: u32 = 2166136261;
    for b in id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    PALETTE[(h % 8) as usize]
}
