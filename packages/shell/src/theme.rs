//! Themes (spec 030): which themes exist besides the built-in `dark`,
//! `light` and `system`, and the tokens of each.

use moonkale_ext_api::Workspace;

/// The names of the themes beyond the built-in ones, for Settings.
pub fn extra_themes(_ws: Workspace) -> Vec<String> {
    Vec::new()
}
