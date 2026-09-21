//! The Extensions panel (Milestone 12, Prompt20): what Settings → Extensions
//! shows, as a panel of its own with an activity-bar entry — built-ins by
//! tier with toggles and permissions, installed wasm modules.

use crate::frame::Extensions_;
use crate::settings_panel::Target;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::settings::SettingsFile;
use std::rc::Rc;

pub const PANEL_ID: &str = "extensions";

pub struct ExtensionsExtension;

impl Extension for ExtensionsExtension {
    fn manifest(&self) -> Manifest {
        Manifest::core(
            "dev.moonkale.extensions",
            "Extensions",
            "The Extensions panel: switch built-in and installed extensions on and off, grant permissions.",
        )
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Extensions".into(),
            home: PanelHome::Main,
            closable: true,
            dirty: false,
            node: None,
            activity: Some(Activity::new("puzzle", 910, "Extensions")),
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { ExtensionsPanel { ws } }
    }
}

#[component]
fn ExtensionsPanel(ws: Workspace) -> Element {
    let mut target = use_signal(|| Target::User);
    rsx! {
        div { class: "mk-settings mk-extensions",
            div { class: "mk-settings-head",
                h2 { "Extensions" }
                span { class: "mk-settings-target",
                    "Changes go to "
                    select {
                        value: if target() == Target::User { "user" } else { "workspace" },
                        onchange: move |e| target.set(if e.value() == "workspace" { Target::Workspace } else { Target::User }),
                        option { value: "user", "user settings" }
                        option { value: "workspace", "this folder" }
                    }
                }
            }
            div { class: "mk-settings-form",
                ExtensionsList { ws, target: target() }
                p { class: "mk-muted",
                    "What each extension is, which tier it belongs to and where it runs: the "
                    a { href: "https://mathstruct.github.io/Moonkale/extensions/Extension-Catalogue", target: "_blank", "Extension Catalogue" }
                    "."
                }
            }
        }
    }
}

/// The list itself, shared with Settings → Extensions.
#[component]
pub fn ExtensionsList(ws: Workspace, target: Target) -> Element {
    let catalog: Rc<Vec<Box<dyn Extension>>> = use_context::<Extensions_>().0;
    let settings = ws.settings.read().clone();
    let ext_target = match target {
        Target::User => moonkale_ext_api::SettingsTarget::User,
        Target::Workspace => moonkale_ext_api::SettingsTarget::Workspace,
    };
    let apply = move |f: Box<dyn FnOnce(&mut SettingsFile)>| {
        spawn(async move {
            match target {
                Target::User => ws.update_user_settings(|file| f(file)).await,
                Target::Workspace => ws.update_workspace_settings(|file| f(file)).await,
            }
        });
    };
    rsx! {
        p { class: "mk-muted", "Optional features load only when switched on. Permissions are what an extension may do; untick to restrict it." }
        for ext in catalog.iter() {
            {
                let m = ext.manifest();
                let id = m.id;
                let on = settings.extensions.is_enabled(&m);
                let granted = settings.extensions.granted(&m);
                let perms: Vec<&'static str> = m.permissions.to_vec();
                rsx! {
                    div { key: "{id}", class: "mk-settings-ext",
                        label { class: "mk-settings-check",
                            input { r#type: "checkbox", checked: on, disabled: !m.optional,
                                onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.extensions.set_enabled(id, v))); } }
                            span { class: "mk-settings-ext-name", "{m.name}" }
                            if !m.optional { span { class: "mk-settings-scope", "core" } }
                            if m.optional && !m.default_enabled { span { class: "mk-settings-scope", "opt-in" } }
                        }
                        div { class: "mk-settings-ext-desc", "{m.description}" }
                        // Milestone 13: the extension's own settings, with the extension.
                        if on {
                            if let Some(section) = ext.settings(ws, ext_target) {
                                div { class: "mk-settings-ext-settings", {section} }
                            }
                        }
                        if !perms.is_empty() {
                            div { class: "mk-settings-ext-perms",
                                for p in perms {
                                    {
                                        let has = granted.iter().any(|g| g == p);
                                        let all: Vec<&'static str> = m.permissions.to_vec();
                                        rsx! {
                                            label { key: "{p}", class: "mk-settings-perm",
                                                input { r#type: "checkbox", checked: has,
                                                    onchange: move |e| {
                                                        let v = e.checked();
                                                        let all = all.clone();
                                                        apply(Box::new(move |f| {
                                                            let cur = f.extensions.permissions.get(id).cloned().unwrap_or_else(|| all.iter().map(|s| s.to_string()).collect());
                                                            let mut next: Vec<String> = cur.into_iter().filter(|c| c != p).collect();
                                                            if v { next.push(p.to_string()); }
                                                            f.extensions.permissions.insert(id.to_string(), next);
                                                        }));
                                                    } }
                                                "{p}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        {
            let wasm = ws.wasm_extensions.read().clone();
            rsx! {
                if !wasm.is_empty() {
                    h4 { class: "mk-settings-sub", "Installed (wasm)" }
                    p { class: "mk-muted", "Third-party modules from ~/.config/moonkale/extensions and <folder>/.moonkale/extensions. Off until enabled; no permission is granted until ticked." }
                }
                for m in wasm {
                    {
                        let id: String = m.id.clone();
                        let on = settings.extensions.is_enabled_id(&id, false);
                        let granted = settings.extensions.permissions.get(&id).cloned().unwrap_or_default();
                        let perms = m.permissions.clone();
                        let id_toggle = id.clone();
                        rsx! {
                            div { key: "{id}", class: "mk-settings-ext",
                                label { class: "mk-settings-check",
                                    input { r#type: "checkbox", checked: on,
                                        onchange: move |e| { let v = e.checked(); let id = id_toggle.clone(); apply(Box::new(move |f| f.extensions.set_enabled(&id, v))); } }
                                    span { class: "mk-settings-ext-name", "{m.name}" }
                                    span { class: "mk-settings-scope", "wasm" }
                                }
                                div { class: "mk-settings-ext-desc", "{m.description} · commands: {m.commands.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(\", \")}" }
                                if !perms.is_empty() {
                                    div { class: "mk-settings-ext-perms",
                                        for p in perms {
                                            {
                                                let has = granted.contains(&p);
                                                let (id2, p2) = (id.clone(), p.clone());
                                                rsx! {
                                                    label { key: "{p}", class: "mk-settings-perm",
                                                        input { r#type: "checkbox", checked: has,
                                                            onchange: move |e| {
                                                                let v = e.checked();
                                                                let (id, p) = (id2.clone(), p2.clone());
                                                                apply(Box::new(move |f| {
                                                                    let cur = f.extensions.permissions.get(&id).cloned().unwrap_or_default();
                                                                    let mut next: Vec<String> = cur.into_iter().filter(|c| *c != p).collect();
                                                                    if v { next.push(p); }
                                                                    f.extensions.permissions.insert(id, next);
                                                                }));
                                                            } }
                                                        "{p}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

    }
}
