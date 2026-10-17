//! Shared controls and palette routes, including the minimal distribution's Rust fallback.
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::{prelude::*, t, EditorAction};

/// Rust commands contributed by the core Code extension when the backend is enabled.
pub fn native_commands(ws: Workspace) -> Vec<CommandContribution> {
    [
        ("native.reload", t!(ws, L, "native-reload"), None),
        ("native.hover", t!(ws, L, "native-hover"), None),
        (
            "native.complete",
            t!(ws, L, "native-complete"),
            Some("Ctrl+Space"),
        ),
        ("native.preview", t!(ws, L, "native-preview"), None),
        ("native.docPreview", t!(ws, L, "native-doc-preview"), None),
        ("native.codeMirror", t!(ws, L, "native-codemirror"), None),
    ]
    .into_iter()
    .map(|(id, title, key)| {
        let mut command = CommandContribution::new(id, title);
        command.keybinding = key.map(str::to_owned);
        command
    })
    .collect()
}

/// Route a Rust command to the active document without coupling adapters.
pub fn run_native_command(id: &str, mut ws: Workspace) {
    let action = match id {
        "native.reload" => EditorAction::NativeReload,
        "native.hover" => EditorAction::NativeHover,
        "native.complete" => EditorAction::NativeComplete,
        "native.preview" => EditorAction::NativePreview,
        "native.docPreview" => EditorAction::NativeDocPreview,
        "native.codeMirror" => EditorAction::NativeCodeMirror,
        _ => return,
    };
    ws.dispatch(Command::Editor(action));
}

const PANEL_CSS: Asset = asset!("/assets/panel.css");

/// Shared wrap and indentation preferences for either editor adapter.
pub fn editor_settings(ws: Workspace, target: SettingsTarget, wrap_label: String) -> Element {
    let settings = ws.settings.resolved.read().editor.clone();
    let spaces_value = settings
        .insert_spaces
        .map(|spaces| if spaces { "spaces" } else { "tabs" })
        .unwrap_or("default");
    rsx! {
        document::Stylesheet { href: PANEL_CSS }
        label { class: "mk-settings-check",
            input { r#type: "checkbox", checked: settings.wrap,
                onchange: move |event| { let value = event.checked(); ws.update_settings_in(target, move |file| file.editor.wrap = Some(value)); } }
            "{wrap_label}"
        }
        label { class: "mk-native-indent-control", {t!(ws, L, "native-indent-style")}
            select { class: "mk-native-indent-style", aria_label: t!(ws, L, "native-indent-style"), value: spaces_value,
                onchange: move |event| {
                    let value = match event.value().as_str() { "spaces" => Some(true), "tabs" => Some(false), _ => None };
                    ws.update_settings_in(target, move |file| file.editor.insert_spaces = value);
                },
                option { value: "default", selected: spaces_value == "default", {t!(ws, L, "native-default")} }
                option { value: "spaces", selected: spaces_value == "spaces", {t!(ws, L, "native-spaces")} }
                option { value: "tabs", selected: spaces_value == "tabs", {t!(ws, L, "native-tabs")} }
            }
        }
        label { class: "mk-native-indent-control", {t!(ws, L, "native-indent-width")}
            select { class: "mk-native-indent-width", aria_label: t!(ws, L, "native-indent-width"),
                value: settings.indent_width.map(|width| width.to_string()).unwrap_or_default(),
                onchange: move |event| { let value = event.value().parse::<u8>().ok(); ws.update_settings_in(target, move |file| file.editor.indent_width = value); },
                option { value: "", selected: settings.indent_width.is_none(), {t!(ws, L, "native-default")} }
                for width in 1..=8 { option { value: "{width}", selected: settings.indent_width == Some(width), "{width}" } }
            }
        }
    }
}
