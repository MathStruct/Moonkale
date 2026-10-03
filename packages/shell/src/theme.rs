//! Themes (spec 030): every colour the app uses is a token, `--mk-<name>`,
//! defined here for Dark and Light and nowhere else (`tools/check-colors.py`
//! keeps the stylesheets free of colours). The shell turns the table into one
//! stylesheet — `:root[data-theme=…]` rules, Follow-system through
//! `prefers-color-scheme` — and sets `data-theme` on `<html>`.
//!
//! More themes come as JSON, from `<config>/moonkale/themes/*.json` (the
//! platform's [`ThemeFiles`] service) or from an extension
//! (`Extension::themes`):
//!
//! ```json
//! { "name": "Solarized Light", "base": "light",
//!   "tokens": { "canvas": "#fdf6e3", "surface": "#fdf6e3", "accent": "#268bd2" } }
//! ```
//!
//! A theme starts from its base and overrides the tokens it names; unknown
//! names are ignored.

use dioxus::prelude::*;
use moonkale_ext_api::{SettingsFuture, Workspace};
use serde::Deserialize;
use std::collections::BTreeMap;

/// `(token, dark, light)`: the whole palette.
pub const TOKENS: &[(&str, &str, &str)] = &[
    ("canvas", "#0f1116", "#f3f4f6"),   // the app's backdrop
    ("surface", "#161922", "#ffffff"),  // panels, popups
    ("sunken", "#0b0d12", "#eceef2"),   // inputs, the title bar, terminals
    ("viewport", "#12141b", "#f8f9fb"), // the editor area
    ("raised", "#1b1e25", "#f4f5f8"),   // boxes inside panels (tool calls, code blocks)
    ("raised-2", "#23272f", "#eaedf2"), // raised items: messages, blocks, hovered rows
    ("input-bg", "#1e222a", "#ffffff"), // text areas
    ("hover", "rgb(255 255 255 / 0.06)", "rgb(0 0 0 / 0.05)"), // hover highlight
    ("pressed", "rgb(255 255 255 / 0.11)", "rgb(0 0 0 / 0.09)"), // pressed highlight
    ("grid", "rgb(255 255 255 / 0.04)", "rgb(0 0 0 / 0.06)"), // table rules
    ("backdrop", "rgb(0 0 0 / 0.45)", "rgb(0 0 0 / 0.25)"), // behind dialogs
    (
        "shadow",
        "0 6px 18px rgb(0 0 0 / 0.5)",
        "0 6px 18px rgb(0 0 0 / 0.15)",
    ), // popups and dialogs
    ("line", "#2a2e3a", "#d9dce3"),     // borders
    ("line-strong", "#3a3f4b", "#c4c8d1"), // emphasised borders
    ("ink", "#e6e8ee", "#1d2129"),      // text
    ("ink-2", "#c0c4cc", "#3a404c"),    // secondary text
    ("ink-soft", "#a3a9b8", "#4f5665"), // quiet text
    ("muted", "#9aa0aa", "#5f6673"),    // hints
    ("ink-faint", "#7f8595", "#747b88"), // faint text
    ("accent", "#6ea8fe", "#2f6fde"),   // the accent
    ("accent-ink", "#0b0d12", "#ffffff"), // text on the accent
    (
        "selected",
        "rgb(110 168 254 / 0.16)",
        "rgb(47 111 222 / 0.14)",
    ), // selected rows
    ("link", "#4fb3e8", "#1677b8"),     // links, wiki-links
    (
        "link-underline",
        "rgb(79 179 232 / 0.5)",
        "rgb(22 119 184 / 0.45)",
    ), // link underlines
    ("good", "#6cb28c", "#2e8a56"),     // success, git added
    ("ok", "#8ccf6a", "#3c8f27"),       // a tool call that ran
    ("caution", "#c9a75f", "#a26f12"),  // git modified, unsaved
    ("warn", "#f2a03d", "#b5650c"),     // notes, approvals
    ("danger", "#e07a75", "#c4382f"),   // errors, git deleted
    (
        "danger-soft",
        "rgb(224 122 117 / 0.14)",
        "rgb(196 56 47 / 0.12)",
    ), // error backgrounds
    ("error-ink", "#e86b8a", "#c0335f"), // failures in the agent and flows
    ("error-ink-soft", "#ffb4b4", "#a8232f"), // error text on error backgrounds
    ("error-bg", "#4a2328", "#fde8ea"), // error messages
    ("approval-bg", "#2a2416", "#fff5e0"), // an approval request
    ("user-msg-bg", "#2d3b55", "#dfe9fb"), // the user's messages
    (
        "diff-add-bg",
        "rgb(108 178 140 / 0.16)",
        "rgb(46 138 86 / 0.14)",
    ), // added lines
    ("diff-add-ink", "#bfe7cf", "#1d6b38"), // added lines' text
    (
        "diff-del-bg",
        "rgb(224 122 117 / 0.16)",
        "rgb(196 56 47 / 0.12)",
    ), // removed lines
    ("diff-del-ink", "#f0b6b2", "#a8232f"), // removed lines' text
    (
        "history-checkpoint",
        "rgb(110 168 254 / 0.08)",
        "rgb(47 111 222 / 0.08)",
    ), // git checkpoints in History
    ("db", "#cc80bf", "#9c3f8e"),       // database files in the Explorer
    ("close-hover", "#c42b1c", "#c42b1c"), // the window's close button
    ("on-danger", "#ffffff", "#ffffff"), // text on red
    ("image-stage", "#0e1014", "#e4e6ea"), // behind images
    ("image-check", "#14161b", "#d6d9de"), // the checkerboard behind images
    ("typst-bg", "#1a1c22", "#e4e6ea"), // behind Typst pages
    ("page", "#ffffff", "#ffffff"),     // a page (Typst output)
    ("code-bg", "#282c34", "#fafafa"),  // the code editor
    ("code-ink", "#abb2bf", "#383a42"), // code
    ("code-cursor", "#528bff", "#526fff"), // the caret
    ("code-selection", "#3e4451", "#e5e5e6"), // selections
    ("code-panel", "#21252b", "#f0f0f1"), // search panel, tooltips
    ("code-gutter", "#7d8799", "#9d9d9f"), // line numbers
    ("code-active-line", "#2c313a", "#f2f2f3"), // the caret's line
    (
        "code-match",
        "rgb(114 161 255 / 0.35)",
        "rgb(82 111 255 / 0.2)",
    ), // search matches
    ("code-match-outline", "#457dff", "#526fff"), // search match outline
    (
        "code-match-current",
        "rgb(97 153 255 / 0.18)",
        "rgb(82 111 255 / 0.32)",
    ), // the current match
    (
        "code-selection-match",
        "rgb(170 254 102 / 0.1)",
        "rgb(80 161 79 / 0.15)",
    ), // other occurrences of the selection
    (
        "code-bracket",
        "rgb(186 208 248 / 0.28)",
        "rgb(64 120 242 / 0.2)",
    ), // matching brackets
    ("syn-keyword", "#c678dd", "#a626a4"), // keywords
    ("syn-name", "#e06c75", "#e45649"), // names, properties
    ("syn-function", "#61afef", "#4078f2"), // functions
    ("syn-constant", "#d19a66", "#986801"), // constants
    ("syn-type", "#e5c07b", "#c18401"), // types, numbers
    ("syn-operator", "#56b6c2", "#0184bc"), // operators, escapes
    ("syn-comment", "#7d8799", "#a0a1a7"), // comments
    ("syn-string", "#98c379", "#50a14f"), // strings
    ("term-bg", "#0b0d12", "#ffffff"),  // terminal background
    ("term-fg", "#e6e8ee", "#1d2129"),  // terminal text
    ("term-cursor", "#6ea8fe", "#2f6fde"), // terminal cursor
    (
        "term-selection",
        "rgb(110 168 254 / 0.3)",
        "rgb(47 111 222 / 0.25)",
    ), // terminal selection
    ("ansi-0", "#3b3f4a", "#383a42"),   // ANSI colour 0
    ("ansi-1", "#e06c75", "#e45649"),   // ANSI colour 1
    ("ansi-2", "#98c379", "#50a14f"),   // ANSI colour 2
    ("ansi-3", "#e5c07b", "#c18401"),   // ANSI colour 3
    ("ansi-4", "#61afef", "#4078f2"),   // ANSI colour 4
    ("ansi-5", "#c678dd", "#a626a4"),   // ANSI colour 5
    ("ansi-6", "#56b6c2", "#0184bc"),   // ANSI colour 6
    ("ansi-7", "#d7dae0", "#a0a1a7"),   // ANSI colour 7
    ("ansi-8", "#7f8595", "#4f525e"),   // ANSI colour 8
    ("ansi-9", "#f28b95", "#e06c75"),   // ANSI colour 9
    ("ansi-10", "#b5e08f", "#66b052"),  // ANSI colour 10
    ("ansi-11", "#f2d493", "#b8860b"),  // ANSI colour 11
    ("ansi-12", "#82c3ff", "#61afef"),  // ANSI colour 12
    ("ansi-13", "#dd97ee", "#c678dd"),  // ANSI colour 13
    ("ansi-14", "#7dd4de", "#56b6c2"),  // ANSI colour 14
    ("ansi-15", "#ffffff", "#1d2129"),  // ANSI colour 15
    ("graph-bg", "#0c0e13", "#f6f7f9"), // behind the graph
    (
        "graph-label",
        "rgb(230 232 238 / 0.85)",
        "rgb(29 33 41 / 0.85)",
    ), // graph labels
    ("graph-label-hover", "#ffffff", "#000000"), // the hovered label
    ("flow-bg", "#14161b", "#f6f7f9"),  // behind the flow canvas
    ("flow-edge", "#8a90a0", "#8a90a0"), // flow wires
    ("flow-port-in", "#4fb3e8", "#1677b8"), // input ports
    ("flow-port-out", "#f2a03d", "#b5650c"), // output ports
];

/// The workbench crate's variables (`--wb-*`) that follow the tokens of the
/// same name.
const WB: &[&str] = &[
    "canvas",
    "surface",
    "sunken",
    "viewport",
    "hover",
    "pressed",
    "line",
    "ink",
    "ink-soft",
    "ink-faint",
    "accent",
    "selected",
    "good",
    "caution",
    "danger",
    "danger-soft",
    "shadow",
];

/// The platform's theme files (spec 030): the JSON texts of
/// `<config>/moonkale/themes/*.json` — read locally on desktop and phone, from
/// the server on the web. Provided in `WorkspaceConfig::services`.
pub struct ThemeFiles(pub fn() -> SettingsFuture<Vec<String>>);

/// A theme beyond Dark and Light.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    /// `"dark"` or `"light"`: where the tokens it does not name come from.
    #[serde(default = "dark")]
    pub base: String,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

fn dark() -> String {
    "dark".into()
}

impl ThemeFile {
    pub fn parse(json: &str) -> Result<Self, String> {
        let t: ThemeFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if t.name.trim().is_empty() || ["dark", "light", "system"].contains(&t.name.as_str()) {
            return Err(format!("a theme needs its own name (not {:?})", t.name));
        }
        Ok(t)
    }

    pub fn is_light(&self) -> bool {
        self.base == "light"
    }
}

/// A token's value is a colour, a length or a shadow — never something that
/// closes the rule (a theme file is data).
fn safe(value: &str) -> bool {
    !value.contains([';', '{', '}', '<', '>']) && value.len() < 120
}

fn block(light: bool, overrides: Option<&BTreeMap<String, String>>) -> String {
    let mut out = String::new();
    for (name, d, l) in TOKENS {
        let value = overrides
            .and_then(|o| o.get(*name))
            .filter(|v| safe(v))
            .map(String::as_str)
            .unwrap_or(if light { l } else { d });
        out.push_str(&format!("--mk-{name}: {value}; "));
    }
    out.push_str(&format!(
        "color-scheme: {};",
        if light { "light" } else { "dark" }
    ));
    out
}

/// The stylesheet for the built-in themes and `extra`.
pub fn css(extra: &[ThemeFile]) -> String {
    let mut out = builtin_css();
    out.push_str(&extra_css(extra));
    out
}

/// The built-in themes' stylesheet — the same on every platform and every
/// render, so it can be part of the server-rendered markup (a `<style>`'s text
/// cannot be patched after hydration: it has no node id, P-152).
pub fn builtin_css() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        ":root, :root[data-theme=\"dark\"] {{ {} }}\n",
        block(false, None)
    ));
    out.push_str(&format!(
        ":root[data-theme=\"light\"] {{ {} }}\n",
        block(true, None)
    ));
    out.push_str(&format!(
        "@media (prefers-color-scheme: light) {{ :root[data-theme=\"system\"] {{ {} }} }}\n",
        block(true, None)
    ));
    // The workbench's own variables follow, wherever it declares them.
    let wb: String = WB
        .iter()
        .map(|n| format!("--wb-{n}: var(--mk-{n}); "))
        .collect();
    // `:root .wb-…` outranks the workbench crate's own palette (declared on
    // `.wb-shell` / `.wb-workspace`, with a dark block of its own).
    out.push_str(&format!(
        ":root .wb-shell, :root .wb-workspace {{ {wb} }}\n"
    ));
    out.push_str("html, body { background: var(--mk-canvas); color: var(--mk-ink); }\n");
    out
}

/// The rules of the themes beyond Dark and Light.
pub fn extra_css(extra: &[ThemeFile]) -> String {
    let mut out = String::new();
    for t in extra {
        let name = t.name.replace(['"', '\\'], "");
        out.push_str(&format!(
            ":root[data-theme=\"{name}\"] {{ {} }}\n",
            block(t.is_light(), Some(&t.tokens))
        ));
    }
    out
}

/// Whether `theme` is light, given the system's preference.
pub fn is_light(theme: &str, system_light: bool, extra: &[ThemeFile]) -> bool {
    match theme {
        "light" => true,
        "dark" => false,
        "system" => system_light,
        name => extra
            .iter()
            .find(|t| t.name == name)
            .is_some_and(ThemeFile::is_light),
    }
}

/// The themes beyond Dark and Light, provided by the frame.
#[derive(Clone, Copy)]
pub struct Themes(pub Signal<Vec<ThemeFile>>);

/// The names of the themes beyond the built-in ones, for Settings.
pub fn extra_themes(_ws: Workspace) -> Vec<String> {
    try_consume_context::<Themes>()
        .map(|t| t.0.read().iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_has_both_values() {
        let mut names: Vec<&str> = TOKENS.iter().map(|t| t.0).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), TOKENS.len(), "duplicate token");
        for (n, d, l) in TOKENS {
            assert!(!d.is_empty() && !l.is_empty(), "{n}");
        }
        for w in WB {
            assert!(TOKENS.iter().any(|t| t.0 == *w), "--wb-{w} has no token");
        }
    }

    #[test]
    fn theme_files_override_their_base_and_cannot_escape() {
        let t = ThemeFile::parse(r##"{"name":"Paper","base":"light","tokens":{"accent":"#ff0000","canvas":"red; } body { display:none"}}"##).unwrap();
        let css = css(&[t]);
        let rule = css.lines().find(|l| l.contains("\"Paper\"")).unwrap();
        assert!(rule.contains("--mk-accent: #ff0000;"));
        // An unsafe value is ignored: the base's canvas stays.
        assert!(rule.contains("--mk-canvas: #f3f4f6;"), "{rule}");
        assert!(rule.contains("color-scheme: light"));
        assert!(ThemeFile::parse(r#"{"name":"dark"}"#).is_err());
        assert!(is_light(
            "Paper",
            false,
            &[ThemeFile::parse(r#"{"name":"Paper","base":"light"}"#).unwrap()]
        ));
        assert!(is_light("system", true, &[]));
        assert!(!is_light("dark", true, &[]));
    }
}
