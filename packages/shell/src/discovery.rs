//! Shortcut discovery uses the registry and the platform state store (ADR-0027).
use crate::{commands::CommandRegistry, L};
use dioxus::prelude::*;
use moonkale_ext_api::{t, Workspace};
use moonkale_state::{Key as StateKey, Record};

pub type CheatSheetState = Signal<bool>;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct MouseUse {
    day: String,
    count: u8,
    shown: bool,
}
impl Record for MouseUse {
    const TABLE: &'static str = "shortcut_nudges";
    const VERSION: u32 = 1;
}
impl MouseUse {
    fn clicked(&mut self, day: &str) -> bool {
        if self.shown {
            return false;
        }
        if self.day != day {
            self.day = day.into();
            self.count = 0;
        }
        self.count = self.count.saturating_add(1).min(3);
        if self.count == 3 {
            self.shown = true;
            true
        } else {
            false
        }
    }
}

/// DOM adapter reports only printable search text and mouse command ids.
/// Consume serially so successive clicks cannot race the read/write pair.
pub fn install(ws: Workspace, mut sheet: CheatSheetState) {
    let mut events = document::eval(include_str!("../assets/discovery.js"));
    let mut palette = consume_context::<crate::palette::PaletteState>();
    let mut seed = consume_context::<crate::palette::SearchSeed>().0;
    let registry = consume_context::<CommandRegistry>();
    spawn(async move {
        while let Ok((kind, value, day)) = events.recv::<(String, String, String)>().await {
            match kind.as_str() {
                "search" if value.chars().count() <= 1 => {
                    seed.set(value);
                    palette.set(Some(crate::palette::PaletteMode::Search));
                }
                "sheet" => sheet.set(true),
                "mouse" if ws.settings.resolved.peek().shortcut_nudges && day.len() <= 10 => {
                    let hint = {
                        let registry = registry.peek();
                        registry
                            .find(&value)
                            .filter(|e| e.binding.is_some())
                            .map(|entry| {
                                moonkale_ext_api::command_hint(&entry.id, entry.title.clone())
                            })
                    };
                    let Some(hint) = hint else { continue };
                    let key = StateKey::new().str(&value);
                    let mut record = ws.state_get::<MouseUse>(&key).await.unwrap_or_default();
                    if record.shown {
                        continue;
                    }
                    let show = record.clicked(&day);
                    ws.state_put(&key, &record).await;
                    if show {
                        let text = t!(ws, L, "discovery-nudge", shortcut = hint.shortcut);
                        let _ = document::eval(&format!(
                            "window.moonkaleShortcutHint?.({});",
                            serde_json::to_string(&text).unwrap_or_default()
                        ));
                    }
                }
                _ => {}
            }
        }
    });
}

#[component]
pub fn CheatSheet() -> Element {
    let ws = use_context::<Workspace>();
    let mut open = use_context::<CheatSheetState>();
    let registry = use_context::<CommandRegistry>();
    let mut query = use_signal(String::new);
    use_effect(move || {
        if open() {
            query.set(String::new());
        }
    });
    if !open() {
        return rsx! {};
    }
    let needle = query().to_lowercase();
    let mut groups = std::collections::BTreeMap::<String, Vec<(String, String, String)>>::new();
    for entry in &registry.read().entries {
        let hint = moonkale_ext_api::command_hint(&entry.id, entry.title.clone());
        if !format!("{} {} {}", entry.title, entry.id, hint.shortcut)
            .to_lowercase()
            .contains(&needle)
        {
            continue;
        }
        let group = entry
            .title
            .split_once(": ")
            .or_else(|| entry.title.split_once('：'))
            .map(|(group, _)| group.to_string())
            .unwrap_or_else(|| t!(ws, L, "discovery-other"));
        groups.entry(group).or_default().push((
            entry.id.clone(),
            entry.title.clone(),
            hint.shortcut,
        ));
    }
    rsx! {
        div { class: "mk-palette-backdrop", onclick: move |_| open.set(false),
            div { class: "mk-palette mk-cheatsheet", role: "dialog", "aria-modal": "true", "aria-label": t!(ws, L, "cmd-help-shortcuts"),
                onclick: |e| e.stop_propagation(),
                onkeydown: move |e| { if e.key() == Key::Escape { open.set(false); e.prevent_default(); } e.stop_propagation(); },
                div { class: "mk-cheatsheet-heading", h2 { {t!(ws, L, "cmd-help-shortcuts")} }
                    button { class: "mk-btn", onclick: move |_| open.set(false), {t!(ws, L, "dismiss")} }
                }
                input { id: "mk-cheatsheet-input", autofocus: true, class: "mk-palette-input", placeholder: t!(ws, L, "discovery-filter"), value: "{query}", oninput: move |e| query.set(e.value()) }
                div { class: "mk-cheatsheet-list",
                    for (group, entries) in groups {
                        section { key: "{group}", h3 { "{group}" }
                            for (id, title, binding) in entries {
                                div { key: "{id}", class: "mk-cheatsheet-row", "data-command-id": "{id}",
                                    span { "{title}" }
                                    kbd { if binding.is_empty() { {t!(ws, L, "settings-unbound")} } else { "{binding}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nudge_is_daily_until_shown_then_never_repeated() {
        let mut record = MouseUse::default();
        assert!(!record.clicked("2026-10-28"));
        assert!(!record.clicked("2026-10-28"));
        assert!(!record.clicked("2026-10-29"));
        assert!(!record.clicked("2026-10-29"));
        assert!(record.clicked("2026-10-29"));
        let bytes = moonkale_state::encode(&record);
        let mut restored: MouseUse = moonkale_state::decode(&bytes).unwrap();
        assert!(!restored.clicked("2026-10-29"));
        assert!(!restored.clicked("2027-01-01"));
    }
}
