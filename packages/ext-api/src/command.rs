//! Commands as a contribution point (Milestone 7): an extension lists what
//! it can do, the shell owns one registry, one keybinding table (overridable
//! from settings) and the palette. Built-in commands are contributions too.

use dioxus::prelude::ReadableExt;
use serde::{Deserialize, Serialize};

/// One command an extension (or the shell) offers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CommandContribution {
    /// Stable, dotted id (`file.save`, `git.commit`). Settings rebind by it.
    pub id: String,
    /// Palette text, e.g. "File: Save".
    pub title: String,
    /// Default keybinding, e.g. `"Ctrl+Shift+P"`; settings override it.
    pub keybinding: Option<String>,
}

impl CommandContribution {
    /// A command with no default keybinding.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            keybinding: None,
        }
    }
    /// Its default keybinding (`"Ctrl+Shift+P"`); the user's settings override it.
    pub fn key(mut self, binding: impl Into<String>) -> Self {
        self.keybinding = Some(binding.into());
        self
    }
}

/// A parsed keybinding: modifiers plus one key name (lower case for
/// characters, `F1`…`F24`, `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`,
/// `Space`, `ArrowUp`… as the DOM names them). `Ctrl` means the platform's
/// primary modifier — Cmd on macOS, Ctrl elsewhere (spec 027) — so one
/// table serves every platform; `crate::keys::primary` decides per event.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Keybinding {
    /// Ctrl (also matched by ⌘ on macOS).
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt (Option on macOS).
    pub alt: bool,
    /// The key: a character in lower case, or a DOM key name (`Enter`, `F2`, `ArrowUp`).
    pub key: String,
}

impl Keybinding {
    /// `"Ctrl+Shift+P"`, `"ctrl+`"`, `"F12"`, `"Alt+ArrowUp"`. Modifier
    /// order and case do not matter; `Cmd`/`Meta` count as `Ctrl`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut kb = Keybinding {
            ctrl: false,
            shift: false,
            alt: false,
            key: String::new(),
        };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        // A trailing "+" key ("Ctrl++") is not supported; "Ctrl+Plus" would be.
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "meta" if !last => kb.ctrl = true,
                "shift" if !last => kb.shift = true,
                "alt" | "option" if !last => kb.alt = true,
                "" => return None,
                _ if last => kb.key = normalize_key(part),
                _ => return None,
            }
        }
        if kb.key.is_empty() {
            None
        } else {
            Some(kb)
        }
    }

    /// Does a DOM `keydown` match? `primary` is the platform's primary
    /// modifier (`crate::keys::primary`), not "Ctrl or Meta".
    pub fn matches(&self, primary: bool, shift: bool, alt: bool, key: &str) -> bool {
        self.ctrl == primary
            && self.shift == shift
            && self.alt == alt
            && self.key == normalize_key(key)
    }

    /// Match punctuation by US physical position, letters by their logical key.
    /// Composition and AltGraph must be rejected by the platform adapter.
    pub fn matches_event(
        &self,
        primary: bool,
        shift: bool,
        alt: bool,
        key: &str,
        code: &str,
    ) -> bool {
        let physical = punctuation_code(&self.key);
        self.ctrl == primary
            && self.shift == shift
            && self.alt == alt
            && match physical {
                Some(position) if !code.is_empty() => position == code,
                _ => self.key == normalize_key(key),
            }
    }

    /// Display a positional binding using the platform's current layout labels.
    pub fn display_with_layout(
        &self,
        layout: &std::collections::BTreeMap<String, String>,
    ) -> String {
        let mut label = self.clone();
        if let Some(value) = punctuation_code(&self.key).and_then(|code| layout.get(code)) {
            label.key = value.clone();
        }
        label.display()
    }

    /// Human form for menus and the palette: `Ctrl+Shift+P` (`Cmd+Shift+P`
    /// on a Mac).
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str(crate::keys::primary_name());
            s.push('+');
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(&display_key(&self.key));
        s
    }
}

/// US punctuation positions used in portable shortcut definitions.
pub fn punctuation_code(key: &str) -> Option<&'static str> {
    Some(match key {
        "/" => "Slash",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "\\" => "Backslash",
        "`" => "Backquote",
        "," => "Comma",
        "." => "Period",
        ";" => "Semicolon",
        "'" => "Quote",
        "-" => "Minus",
        "=" => "Equal",
        _ => return None,
    })
}

fn normalize_key(key: &str) -> String {
    match key {
        " " | "Space" | "space" => "Space".into(),
        "Esc" | "esc" => "Escape".into(),
        k if k.chars().count() == 1 => k.to_ascii_lowercase(),
        k => {
            // Named keys keep their DOM spelling, case-insensitively.
            let lower = k.to_ascii_lowercase();
            for name in [
                "Enter",
                "Escape",
                "Tab",
                "Backspace",
                "Delete",
                "Space",
                "ArrowUp",
                "ArrowDown",
                "ArrowLeft",
                "ArrowRight",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "Insert",
            ] {
                if name.to_ascii_lowercase() == lower {
                    return name.into();
                }
            }
            if let Some(n) = lower.strip_prefix('f') {
                if n.parse::<u8>().is_ok() {
                    return format!("F{n}");
                }
            }
            k.to_string()
        }
    }
}

fn display_key(key: &str) -> String {
    if key.chars().count() == 1 {
        key.to_ascii_uppercase()
    } else {
        key.to_string()
    }
}

/// Subsequence fuzzy match: every query character must appear in order;
/// bonuses for word starts and adjacency, a penalty for gaps. Higher is
/// better; `None` when the query does not match. Case-insensitive.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let t: Vec<char> = text.chars().collect();
    let tl: Vec<char> = t.iter().flat_map(|c| c.to_lowercase()).collect();
    let mut score = 0i32;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, c) in tl.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            score += 10;
            let word_start = i == 0
                || !t[i - 1].is_alphanumeric()
                || (t[i].is_uppercase() && t[i - 1].is_lowercase());
            if word_start {
                score += 15;
            }
            if let Some(l) = last {
                if l + 1 == i {
                    score += 10;
                } else {
                    score -= ((i - l - 1) as i32).min(20);
                }
            } else {
                score -= (i as i32).min(20);
            }
            last = Some(i);
            qi += 1;
        }
    }
    if qi == q.len() {
        // Shorter texts win ties.
        Some(score - (t.len() as i32) / 8)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keybinding_parse_and_match() {
        let kb = Keybinding::parse("Ctrl+Shift+P").unwrap();
        assert!(kb.matches(true, true, false, "P"));
        assert!(kb.matches(true, true, false, "p"));
        assert!(!kb.matches(true, false, false, "p"));
        // macOS shows the primary modifier as Cmd (spec 027).
        let primary = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        assert_eq!(kb.display(), format!("{primary}+Shift+P"));
        let kb = Keybinding::parse("cmd+`").unwrap();
        assert!(kb.ctrl && kb.key == "`");
        assert!(Keybinding::parse("F12")
            .unwrap()
            .matches(false, false, false, "F12"));
        assert!(Keybinding::parse("ctrl+,")
            .unwrap()
            .matches(true, false, false, ","));
        assert_eq!(Keybinding::parse("Alt+arrowup").unwrap().key, "ArrowUp");
        assert!(Keybinding::parse("Ctrl+").is_none());
        assert!(Keybinding::parse("Bogus+P").is_none());
    }

    #[test]
    fn fuzzy_prefers_word_starts_and_adjacency() {
        assert!(fuzzy_score("xyz", "File: Save").is_none());
        let a = fuzzy_score("fs", "File: Save").unwrap();
        let b = fuzzy_score("fs", "Refresh Stuff").unwrap();
        assert!(a > b, "{a} vs {b}");
        let exact = fuzzy_score("save", "File: Save").unwrap();
        let spread = fuzzy_score("save", "Show all views everywhere").unwrap();
        assert!(exact > spread);
        assert_eq!(fuzzy_score("", "anything"), Some(0));
        assert!(fuzzy_score("mainrs", "src/main.rs").is_some());
    }
}

/// Effective command bindings supplied reactively by the shell. An empty value
/// means unbound; panel components never guess defaults from a command's name.
#[derive(Clone, Copy, PartialEq)]
pub struct CommandBindings(pub dioxus::prelude::Signal<std::collections::BTreeMap<String, String>>);

/// One command's presentation, shared by buttons, menus, hints and palettes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandHint {
    /// Current platform-formatted binding, empty when unbound.
    pub shortcut: String,
    /// Localized action name with its binding in parentheses when bound.
    pub title: String,
}

/// Read the shell's current keymap in a component. Without a shell context the
/// action stays named but unbound, so standalone panel hosts remain supported.
pub fn command_hint(id: &str, label: impl Into<String>) -> CommandHint {
    let shortcut = dioxus::prelude::try_consume_context::<CommandBindings>()
        .and_then(|bindings| bindings.0.read().get(id).cloned())
        .unwrap_or_default();
    let label = label.into();
    let title = if shortcut.is_empty() {
        label
    } else {
        format!("{label} ({shortcut})")
    };
    CommandHint { shortcut, title }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use dioxus::prelude::*;

    #[test]
    fn filtering_command_rows_preserves_component_hook_order() {
        thread_local! {
            static COUNT: std::cell::Cell<Option<Signal<usize>>> = const { std::cell::Cell::new(None) };
            static RENDERS: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        fn app() -> Element {
            let count = use_signal(|| 2usize);
            use_context_provider(|| {
                CommandBindings(Signal::new(std::collections::BTreeMap::new()))
            });
            for _ in 0..count() {
                let _ = command_hint("file.save", "Save");
            }
            // A hook after the variable-length command list must retain its slot.
            let marker = use_signal(|| 41usize);
            COUNT.with(|value| value.set(Some(count)));
            RENDERS.with(|values| values.borrow_mut().push(marker()));
            rsx! {}
        }
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        for count in [1, 3, 0] {
            dom.in_scope(ScopeId::ROOT, || {
                COUNT.with(|value| value.get().unwrap().set(count))
            });
            dom.render_immediate(&mut dioxus_core::NoOpMutations);
        }
        RENDERS.with(|values| assert_eq!(*values.borrow(), vec![41; 4]));
    }

    #[test]
    fn presentation_reads_rebinding_unbinding_and_missing_context() {
        fn app() -> Element {
            assert_eq!(command_hint("file.save", "Save").title, "Save");
            let mut bindings = use_context_provider(|| {
                CommandBindings(Signal::new(std::collections::BTreeMap::from([(
                    "file.save".into(),
                    "Ctrl+S".into(),
                )])))
            })
            .0;
            assert_eq!(command_hint("file.save", "Save").title, "Save (Ctrl+S)");
            bindings.set(std::collections::BTreeMap::from([(
                "file.save".into(),
                "Alt+S".into(),
            )]));
            assert_eq!(
                command_hint("file.save", "Speichern").title,
                "Speichern (Alt+S)"
            );
            bindings.set(std::collections::BTreeMap::from([(
                "file.save".into(),
                String::new(),
            )]));
            assert_eq!(command_hint("file.save", "保存").title, "保存");
            assert_eq!(command_hint("absent", "Action").shortcut, "");
            rsx! {}
        }
        VirtualDom::new(app).rebuild_in_place();
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn punctuation_is_physical_and_letters_are_logical() {
        let slash = Keybinding::parse("Ctrl+/").unwrap();
        for key in ["/", "-", ":", "z"] {
            assert!(slash.matches_event(true, false, false, key, "Slash"));
        }
        assert!(!slash.matches_event(true, false, false, "/", "Digit7"));
        assert!(!slash.matches_event(true, false, true, "/", "Slash"));
        let undo = Keybinding::parse("Ctrl+Z").unwrap();
        assert!(undo.matches_event(true, false, false, "z", "KeyY"));
        assert!(!undo.matches_event(true, false, false, "y", "KeyZ"));
        let layout = std::collections::BTreeMap::from([("Slash".into(), "-".into())]);
        assert!(slash.display_with_layout(&layout).ends_with("+-"));
        for n in 13..=24 {
            let text = format!("F{n}");
            assert!(Keybinding::parse(&text)
                .unwrap()
                .matches_event(false, false, false, &text, &text));
        }
    }
}
