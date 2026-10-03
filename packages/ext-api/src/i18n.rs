//! An extension's strings (Milestone 18 phase 4.4, preparing spec 030):
//! each extension brings its own text per language, the way it brings its
//! settings — so translating later touches no extension's API again.
//!
//! A locale file is Fluent (`.ftl`) restricted for now to simple messages,
//! `key = text`, one per line, `#` comments; the lookup here reads exactly
//! that. Spec 030 swaps in `fluent-bundle` (variables, plurals) behind the
//! same files and the same [`Locales`] contribution. Keys, not English text,
//! are the ids.

/// `(language tag, Fluent source)` pairs, e.g.
/// `&[("en", include_str!("../locales/en.ftl")), ("de", include_str!("../locales/de.ftl"))]`.
pub type Locales = &'static [(&'static str, &'static str)];

/// The language every extension must have, and the fallback.
pub const FALLBACK: &str = "en";

/// The text of `key` in `lang` (`"de"`, `"zh-CN"`; a region falls back to
/// its language), else in English; `None` when no locale has it.
pub fn lookup(locales: Locales, lang: &str, key: &str) -> Option<&'static str> {
    let base = lang.split(['-', '_']).next().unwrap_or(lang);
    [lang, base, FALLBACK]
        .iter()
        .find_map(|l| locales.iter().find(|(tag, _)| tag.eq_ignore_ascii_case(l)))
        .into_iter()
        .chain(locales.iter().filter(|(tag, _)| *tag == FALLBACK))
        .find_map(|(_, src)| message(src, key))
}

/// A simple message of a Fluent source.
fn message(src: &'static str, key: &str) -> Option<&'static str> {
    src.lines().find_map(|line| {
        let line = line.trim_start();
        if line.starts_with('#') {
            return None;
        }
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Locales = &[
        (
            "en",
            "# the panel\npanel-title = History\ncompact = Compact\n",
        ),
        ("de", "panel-title = Verlauf\n"),
        ("zh-CN", "panel-title = 历史\n"),
    ];

    #[test]
    fn language_region_then_english() {
        assert_eq!(lookup(L, "de", "panel-title"), Some("Verlauf"));
        assert_eq!(lookup(L, "de-AT", "panel-title"), Some("Verlauf"));
        assert_eq!(lookup(L, "zh-CN", "panel-title"), Some("历史"));
        // Missing in German: English.
        assert_eq!(lookup(L, "de", "compact"), Some("Compact"));
        // Unknown language: English.
        assert_eq!(lookup(L, "fr", "panel-title"), Some("History"));
        assert_eq!(lookup(L, "en", "missing"), None);
        assert_eq!(
            lookup(L, "en", "the panel"),
            None,
            "comments are not messages"
        );
    }
}
