//! Commands as a contribution point (Milestone 7): an extension lists what
//! it can do, the shell owns one registry, one keybinding table (overridable
//! from settings) and the palette. Built-in commands are contributions too.

use serde::{Deserialize, Serialize};

/// One command an extension (or the shell) offers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContribution {
    /// Stable, dotted id (`file.save`, `git.commit`). Settings rebind by it.
    pub id: String,
    /// Palette text, e.g. "File: Save".
    pub title: String,
    /// Default keybinding, e.g. `"Ctrl+Shift+P"`; settings override it.
    pub keybinding: Option<String>,
}

impl CommandContribution {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            keybinding: None,
        }
    }
    pub fn key(mut self, binding: impl Into<String>) -> Self {
        self.keybinding = Some(binding.into());
        self
    }
}

/// A parsed keybinding: modifiers plus one key name (lower case for
/// characters, `F1`…`F12`, `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`,
/// `Space`, `ArrowUp`… as the DOM names them). `Ctrl` means the platform's
/// primary modifier — Cmd on macOS, Ctrl elsewhere (spec 027) — so one
/// table serves every platform; `crate::keys::primary` decides per event.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Keybinding {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
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
        assert_eq!(kb.display(), "Ctrl+Shift+P");
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
