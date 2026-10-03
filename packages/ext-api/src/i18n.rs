//! An extension's strings in the user's language (Milestone 18 phase 4.4;
//! built for spec 030). Each extension brings its own text per language —
//! a contribution, like its settings — as **Fluent** sources (`.ftl`):
//! `key = text`, variables `{ $n }`, plural selectors for German, nothing
//! special for Chinese. Keys, not English text, are the ids, so a changed
//! English wording does not orphan a translation.
//!
//! ```
//! use moonkale_ext_api::i18n::{tr, Locales, FluentArgs};
//! static L: Locales = &[
//!     ("en", "files = { $n ->\n    [one] { $n } file\n   *[other] { $n } files\n}\n"),
//!     ("de", "files = { $n ->\n    [one] { $n } Datei\n   *[other] { $n } Dateien\n}\n"),
//! ];
//! let mut a = FluentArgs::new();
//! a.set("n", 2);
//! assert_eq!(tr(L, "de", "files", Some(&a)), "2 Dateien");
//! assert_eq!(tr(L, "fr", "files", Some(&a)), "2 files");
//! ```
//!
//! In a component, [`crate::t!`] reads the workspace's language (so the
//! component re-renders when it changes): `t!(ws, L, "files", n = 2)`.

use fluent_bundle::{FluentBundle, FluentResource};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub use fluent_bundle::{FluentArgs, FluentValue};

/// `(language tag, Fluent source)` pairs, e.g.
/// `&[("en", include_str!("../locales/en.ftl")), ("de", include_str!("../locales/de.ftl"))]`.
pub type Locales = &'static [(&'static str, &'static str)];

/// The language every extension must have, and the fallback.
pub const FALLBACK: &str = "en";

/// The languages Moonkale ships, with their own names (Settings → You).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "Deutsch"),
    ("zh-CN", "中文（简体）"),
];

type Bundle = Rc<FluentBundle<FluentResource>>;

thread_local! {
    static BUNDLES: RefCell<HashMap<(usize, String), Option<Bundle>>> = RefCell::new(HashMap::new());
}

/// The bundle of one language of one locale table (cached per thread).
fn bundle(locales: Locales, tag: &str) -> Option<Bundle> {
    let key = (locales.as_ptr() as usize, tag.to_string());
    if let Some(b) = BUNDLES.with(|c| c.borrow().get(&key).cloned()) {
        return b;
    }
    let made = locales
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(tag))
        .and_then(|(t, src)| {
            let res = FluentResource::try_new(src.to_string()).unwrap_or_else(|(res, _errors)| res);
            let lang: unic_langid::LanguageIdentifier = t.parse().ok()?;
            let mut b = FluentBundle::new(vec![lang]);
            // No bidi isolation marks around variables: the UI is LTR, and
            // the marks would end up in copied text.
            b.set_use_isolating(false);
            b.add_resource(res).ok()?;
            Some(Rc::new(b))
        });
    BUNDLES.with(|c| c.borrow_mut().insert(key, made.clone()));
    made
}

/// The languages to try for `lang`: itself, its base (`de-AT` → `de`), the
/// region-qualified tag of a bare language we ship (`zh` → `zh-CN`), English.
fn chain(lang: &str) -> Vec<String> {
    let lang = lang.replace('_', "-");
    let base = lang.split('-').next().unwrap_or(&lang).to_string();
    let mut out = vec![lang.clone(), base.clone()];
    for (tag, _) in LANGUAGES {
        if tag.split('-').next() == Some(base.as_str()) {
            out.push(tag.to_string());
        }
    }
    out.push(FALLBACK.into());
    out.dedup();
    out
}

/// The text of `key` in `lang`, formatted with `args`; else in English;
/// else the key itself (so a missing string is visible, not blank).
pub fn tr(locales: Locales, lang: &str, key: &str, args: Option<&FluentArgs>) -> String {
    for tag in chain(lang) {
        let Some(b) = bundle(locales, &tag) else {
            continue;
        };
        let Some(pattern) = b.get_message(key).and_then(|m| m.value()) else {
            continue;
        };
        let mut errors = Vec::new();
        return b.format_pattern(pattern, args, &mut errors).into_owned();
    }
    key.to_string()
}

/// The text of a simple message (`key = text`, no variables) in `lang`,
/// falling back to English (`None`: no locale has it). Kept from `lib-v1`;
/// prefer [`tr`].
pub fn lookup(locales: Locales, lang: &str, key: &str) -> Option<&'static str> {
    chain(lang).iter().find_map(|tag| {
        let (_, src) = locales.iter().find(|(t, _)| t.eq_ignore_ascii_case(tag))?;
        src.lines().find_map(|line| {
            let line = line.trim_start();
            if line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim())
        })
    })
}

/// The system's language (`LANG` and the OS on desktop, the device locale on
/// Android, `navigator.language` in the browser), as a tag; English when it
/// cannot be told.
pub fn system_language() -> String {
    sys_locale::get_locale()
        .map(|l| l.replace('_', "-"))
        .map(|l| l.split('.').next().unwrap_or(&l).to_string())
        .filter(|l| !l.is_empty() && l != "C" && l != "POSIX")
        .unwrap_or_else(|| FALLBACK.into())
}

/// What is wrong with a locale table: a language whose file does not parse,
/// and keys present in one language but missing in another. Every
/// extension's tests call this on its table.
pub fn check(locales: Locales) -> Vec<String> {
    let mut problems = Vec::new();
    let mut keys: Vec<(&str, std::collections::BTreeSet<String>)> = Vec::new();
    for (tag, src) in locales {
        match FluentResource::try_new(src.to_string()) {
            Ok(res) => {
                let set = res
                    .entries()
                    .filter_map(|e| match e {
                        fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_string()),
                        _ => None,
                    })
                    .collect();
                keys.push((tag, set));
            }
            Err((_, errors)) => problems.push(format!(
                "{tag}: {} parse error(s): {:?}",
                errors.len(),
                errors.first()
            )),
        }
    }
    if !locales.iter().any(|(t, _)| *t == FALLBACK) {
        problems.push("no English (en) locale".into());
    }
    for (tag, set) in &keys {
        for (other, oset) in &keys {
            for k in oset.difference(set) {
                problems.push(format!("{tag}: missing `{k}` (in {other})"));
            }
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

/// `t!(ws, LOCALES, "key")` or `t!(ws, LOCALES, "key", n = 3, name = "x")`:
/// the text in the workspace's language (reading it subscribes the
/// component, so it re-renders when the language changes).
#[macro_export]
macro_rules! t {
    ($ws:expr, $loc:expr, $key:literal) => {
        $crate::i18n::tr($loc, &$ws.lang(), $key, None)
    };
    ($ws:expr, $loc:expr, $key:literal, $($name:ident = $val:expr),+ $(,)?) => {{
        let mut args = $crate::i18n::FluentArgs::new();
        $( args.set(stringify!($name), $val); )+
        $crate::i18n::tr($loc, &$ws.lang(), $key, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Locales = &[
        ("en", "# the panel\npanel-title = History\ncompact = Compact { $n ->\n    [one] one event\n   *[other] { $n } events\n}\n"),
        ("de", "panel-title = Verlauf\ncompact = { $n ->\n    [one] Ein Ereignis\n   *[other] { $n } Ereignisse\n} verdichten\n"),
        ("zh-CN", "panel-title = 历史\n"),
    ];

    #[test]
    fn language_base_region_then_english_then_the_key() {
        assert_eq!(tr(L, "de", "panel-title", None), "Verlauf");
        assert_eq!(tr(L, "de-AT", "panel-title", None), "Verlauf");
        assert_eq!(tr(L, "de_DE.UTF-8", "panel-title", None), "Verlauf");
        assert_eq!(tr(L, "zh", "panel-title", None), "历史");
        assert_eq!(tr(L, "zh-CN", "panel-title", None), "历史");
        assert_eq!(tr(L, "fr", "panel-title", None), "History");
        assert_eq!(tr(L, "en", "missing", None), "missing");
        assert_eq!(lookup(L, "de", "panel-title"), Some("Verlauf"));
    }

    #[test]
    fn plurals_per_language() {
        let mut a = FluentArgs::new();
        a.set("n", 1);
        assert_eq!(tr(L, "de", "compact", Some(&a)), "Ein Ereignis verdichten");
        a.set("n", 5);
        assert_eq!(tr(L, "de", "compact", Some(&a)), "5 Ereignisse verdichten");
        assert_eq!(tr(L, "en", "compact", Some(&a)), "Compact 5 events");
        // Chinese has no translation for it: English.
        assert_eq!(tr(L, "zh-CN", "compact", Some(&a)), "Compact 5 events");
    }

    #[test]
    fn check_finds_missing_keys() {
        let p = check(L);
        assert!(
            p.contains(&"zh-CN: missing `compact` (in en)".to_string()),
            "{p:?}"
        );
        assert!(!p.iter().any(|x| x.starts_with("en:")), "{p:?}");
    }
}
