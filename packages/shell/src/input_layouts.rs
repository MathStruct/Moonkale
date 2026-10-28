//! Platform input adapter; the command registry remains the source of truth.
use dioxus::prelude::*;
use moonkale_ext_api::Workspace;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Input {
    Layout { labels: BTreeMap<String, String> },
    Command { id: String },
    Mouse { id: String },
}

pub fn install(ws: Workspace, mut layout: Signal<BTreeMap<String, String>>) {
    let mut events = document::eval(include_str!("../assets/input-layouts.js"));
    spawn(async move {
        while let Ok(event) = events.recv::<Input>().await {
            match event {
                Input::Layout { labels }
                    if labels.len() <= 256
                        && labels.iter().all(|(k, v)| k.len() <= 32 && v.len() <= 64) =>
                {
                    layout.set(labels)
                }
                Input::Command { id } if id.len() <= 128 => {
                    crate::commands::run(&id, ws);
                }
                Input::Mouse { id } => {
                    let command = crate::commands::mouse_command(ws, &id);
                    crate::commands::run(&command, ws);
                }
                _ => {}
            }
        }
    });
}

pub fn publish(ws: Workspace, registry: &crate::commands::Registry) {
    let bindings: Vec<_> = registry.entries.iter().filter_map(|entry| {
        let binding = entry.binding.as_ref()?;
        Some(serde_json::json!({"id":entry.id, "ctrl":binding.ctrl, "shift":binding.shift,
            "alt":binding.alt, "key":binding.key, "code":moonkale_ext_api::punctuation_code(&binding.key)}))
    }).collect();
    let mouse: BTreeMap<_, _> = ["back", "forward", "middle"]
        .into_iter()
        .map(|name| (name, crate::commands::mouse_command(ws, name)))
        .collect();
    let _ = document::eval(&format!("window.moonkaleInputBindings = {}; window.moonkaleMouseBindings = {}; document.documentElement.dataset.keymap = {};", serde_json::to_string(&bindings).unwrap(), serde_json::to_string(&mouse).unwrap(), serde_json::to_string(&ws.settings.resolved.read().keymap).unwrap()));
}
