//! The Extensions panel (Milestone 12, Prompt20): what Settings → Extensions
//! shows, as a panel of its own with an activity-bar entry — built-ins by
//! tier with toggles and permissions, installed wasm modules.

use crate::frame::Extensions_;
use crate::settings_panel::Target;
use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::settings::SettingsFile;
use moonkale_ext_api::t;
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

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        vec![
            PanelContribution::new(PANEL_ID, t!(ws, L, "extensions-title"), PanelHome::Main)
                .closable(true)
                .activity(Activity::new("puzzle", 910, t!(ws, L, "extensions-title"))),
        ]
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
                h2 { {t!(ws, L, "extensions-title")} }
                span { class: "mk-settings-target",
                    {t!(ws, L, "extensions-changes-go-to")}
                    select {
                        value: if target() == Target::User { "user" } else { "workspace" },
                        onchange: move |e| target.set(if e.value() == "workspace" { Target::Workspace } else { Target::User }),
                        option { value: "user", {t!(ws, L, "extensions-user-settings")} }
                        option { value: "workspace", {t!(ws, L, "extensions-this-folder")} }
                    }
                }
            }
            div { class: "mk-settings-form",
                ExtensionsList { ws, target: target() }
                p { class: "mk-muted",
                    {t!(ws, L, "extensions-catalogue-before")}
                    a { href: "https://mathstruct.github.io/Moonkale/extensions/Extension-Catalogue", target: "_blank", {t!(ws, L, "extensions-catalogue")} }
                    {t!(ws, L, "extensions-catalogue-after")}
                }
            }
        }
    }
}

/// The list itself, shared with Settings → Extensions.
#[component]
pub fn ExtensionsList(ws: Workspace, target: Target) -> Element {
    let catalog: Rc<Vec<Box<dyn Extension>>> = use_context::<Extensions_>().0;
    let settings = ws.settings.resolved.read().clone();
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
    // Grants are the user's alone (Milestone 18 phase 4.5, audit #8): a
    // folder's settings may not grant, so a change here always goes to the
    // user scope, whichever file the switch above names.
    let grant = move |f: Box<dyn FnOnce(&mut SettingsFile)>| {
        spawn(async move { ws.update_user_settings(|file| f(file)).await });
    };
    rsx! {
        p { class: "mk-muted", {t!(ws, L, "extensions-hint")} }
        for ext in catalog.iter() {
            {
                let m = ext.manifest();
                let id = m.id;
                let on = settings.extensions.is_enabled(&m);
                let granted = settings.extensions.granted(&m);
                let perms: Vec<&'static str> = m.permissions.to_vec();
                // Spec 030: the extension's own strings name it, if it has them.
                let lang = ws.lang();
                let own = |key: &str, fallback: &str| {
                    let s = moonkale_ext_api::i18n::tr(ext.locales(), &lang, key, None);
                    if s == key { fallback.to_string() } else { s }
                };
                let (name, description) = (own("extension-name", m.name), own("extension-description", m.description));
                rsx! {
                    div { key: "{id}", class: "mk-settings-ext",
                        label { class: "mk-settings-check",
                            input { r#type: "checkbox", checked: on, disabled: !m.optional,
                                onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.extensions.set_enabled(id, v))); } }
                            span { class: "mk-settings-ext-name", "{name}" }
                            if !m.optional { span { class: "mk-settings-scope", {t!(ws, L, "extensions-core")} } }
                            if m.optional && !m.default_enabled { span { class: "mk-settings-scope", {t!(ws, L, "extensions-opt-in")} } }
                        }
                        div { class: "mk-settings-ext-desc", "{description}" }
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
                                                        grant(Box::new(move |f| {
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
            let wasm = ws.contrib.wasm_extensions.read().clone();
            rsx! {
                if !wasm.is_empty() {
                    h4 { class: "mk-settings-sub", {t!(ws, L, "extensions-wasm")} }
                    p { class: "mk-muted", {t!(ws, L, "extensions-wasm-hint")} }
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
                                div { class: "mk-settings-ext-desc", {t!(ws, L, "extensions-wasm-desc", description = m.description.clone(), commands = m.commands.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(", "))} }
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
                                                                grant(Box::new(move |f| {
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
