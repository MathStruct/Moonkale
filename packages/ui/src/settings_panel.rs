//! Settings panel (Milestone 5): the resolved settings as a form, each
//! field with a scope switch (user / workspace), plus the raw JSON of both
//! files. Changes apply live and persist on change.

use crate::frame::Extensions_;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::settings::{Scope, SettingsFile};
use std::rc::Rc;

pub const PANEL_ID: &str = "settings";

pub struct SettingsExtension;

impl Extension for SettingsExtension {
    fn manifest(&self) -> Manifest {
        Manifest::core(
            "dev.moonkale.settings",
            "Settings",
            "The Settings panel (Ctrl+,).",
        )
    }

    fn panels(&self, _ws: Workspace) -> Vec<PanelContribution> {
        vec![PanelContribution {
            id: PANEL_ID.into(),
            title: "Settings".into(),
            home: PanelHome::Main,
            closable: true,
            dirty: false,
            node: None,
            activity: Some(Activity::new("settings", 900, "Settings")),
        }]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { SettingsPanel { ws } }
    }
}

/// Which file a change goes to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    User,
    Workspace,
}

fn scope_label(s: Scope) -> &'static str {
    match s {
        Scope::Default => "default",
        Scope::User => "user",
        Scope::Workspace => "workspace",
        Scope::Env => "env",
    }
}

#[component]
fn SettingsPanel(ws: Workspace) -> Element {
    let mut target = use_signal(|| Target::User);
    let mut tab = use_signal(|| "form");
    let mut secret_name = use_signal(String::new);
    let mut secret_value = use_signal(String::new);
    let mut secret_status = use_signal(String::new);

    let catalog: Rc<Vec<Box<dyn Extension>>> = use_context::<Extensions_>().0;
    let registry = use_context::<crate::commands::CommandRegistry>();
    let settings = ws.settings.read().clone();
    let user = ws.settings_user.read().clone();
    let workspace = ws.settings_workspace.read().clone();
    let env = moonkale_ext_api::settings::Settings::env_overrides();
    let has_workspace = ws.settings_folder.read().is_some();
    let llm_scope = moonkale_ext_api::settings::Settings::scope_of_llm(&user, &workspace, &env);
    let store_available = ws.has_settings_store();

    // Apply a change to the chosen scope's file.
    let apply = move |f: Box<dyn FnOnce(&mut SettingsFile)>| {
        let t = *target.peek();
        spawn(async move {
            match t {
                Target::User => ws.update_user_settings(|file| f(file)).await,
                Target::Workspace => ws.update_workspace_settings(|file| f(file)).await,
            }
        });
    };

    let provider = settings.llm.provider.clone();
    let model = settings.llm.model.clone();
    let base_url = settings.llm.base_url.clone();
    let embed_model = settings.llm.embed_model.clone().unwrap_or_default();
    let secret = settings.llm.secret.clone();
    let denied = settings.policy.denied_tools.join(", ");

    let opt = |s: String| {
        if s.trim().is_empty() {
            None
        } else {
            Some(s.trim().to_string())
        }
    };

    rsx! {
        div { class: "mk-settings",
            div { class: "mk-settings-toolbar",
                span { class: "mk-settings-tabs",
                    button { class: if tab() == "form" { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| tab.set("form"), "Form" }
                    button { class: if tab() == "json" { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| tab.set("json"), "JSON" }
                }
                span { class: "mk-settings-spacer" }
                span { class: "mk-muted", "changes go to: " }
                button { class: if target() == Target::User { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| target.set(Target::User), title: "This machine (all folders)", "user" }
                button { class: if target() == Target::Workspace { "mk-btn mk-btn-on" } else { "mk-btn" }, disabled: !has_workspace, onclick: move |_| target.set(Target::Workspace), title: "This folder (.moonkale/settings.json)", "workspace" }
            }
            if !store_available {
                p { class: "mk-settings-note", "User settings are not persisted on this platform; workspace settings still are." }
            }
            if tab() == "form" {
                div { class: "mk-settings-form",
                    h3 { "Language model" span { class: "mk-settings-scope", "{scope_label(llm_scope)}" } }
                    label { "Provider"
                        select { class: "mk-input", value: "{provider}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.provider = Some(v))); },
                            for (k, name) in [("mock", "mock (offline)"), ("anthropic", "Anthropic"), ("openai", "OpenAI-compatible (OpenAI, Mistral, …)"), ("ollama", "Ollama (local)")] {
                                option { value: "{k}", selected: provider == k, "{name}" }
                            }
                        }
                    }
                    label { "Model"
                        input { class: "mk-input", value: "{model}", placeholder: "provider default",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.model = opt(v))); } }
                    }
                    label { "Endpoint"
                        input { class: "mk-input", value: "{base_url}", placeholder: "https://api.openai.com/v1 · https://api.mistral.ai/v1 · http://127.0.0.1:11434",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.base_url = opt(v))); } }
                    }
                    label { "Embedding model"
                        input { class: "mk-input", value: "{embed_model}", placeholder: "none (keyword search only)",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.embed_model = opt(v))); } }
                    }
                    label { "Secret name"
                        input { class: "mk-input", value: "{secret}", placeholder: "defaults to the provider name",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.secret = opt(v))); } }
                    }
                    p { class: "mk-muted", "Keys are never stored in settings. They come from MOONKALE_SECRET_<NAME>, the classic ANTHROPIC_API_KEY / OPENAI_API_KEY, or the secrets file written below (desktop) / on the server (web)." }
                    if ws.secret_store().is_some() {
                        div { class: "mk-settings-secret",
                            input { class: "mk-input", placeholder: "secret name (e.g. openai)", value: "{secret_name}", oninput: move |e| secret_name.set(e.value()) }
                            input { class: "mk-input", r#type: "password", placeholder: "API key", value: "{secret_value}", oninput: move |e| secret_value.set(e.value()) }
                            button { class: "mk-btn", disabled: secret_name().trim().is_empty(), onclick: move |_| {
                                let name = secret_name.peek().trim().to_string();
                                let value = secret_value.peek().clone();
                                let store = ws.secret_store().unwrap();
                                spawn(async move {
                                    match store(name.clone(), value).await {
                                        Ok(()) => { secret_status.set(format!("stored secret {name:?}")); secret_value.set(String::new()); }
                                        Err(e) => secret_status.set(format!("not stored: {e}")),
                                    }
                                });
                            }, "Store secret" }
                            span { class: "mk-muted", "{secret_status}" }
                        }
                    }

                    h3 { "Agent policy" }
                    label { class: "mk-settings-check",
                        input { r#type: "checkbox", checked: settings.policy.allow_writes,
                            onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.policy.allow_writes = Some(v))); } }
                        "Allow mutating tools without asking (destructive ones always ask)"
                    }
                    label { "Denied tools"
                        input { class: "mk-input", value: "{denied}", placeholder: "comma-separated tool names",
                            onchange: move |e| { let v: Vec<String> = e.value().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(); apply(Box::new(move |f| f.policy.denied_tools = Some(v))); } }
                    }

                    h3 { "Search" }
                    label { class: "mk-settings-check",
                        input { r#type: "checkbox", checked: settings.search.embeddings,
                            onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.search.embeddings = Some(v))); } }
                        "Use embeddings when an embedding model is configured (applies to folders opened afterwards)"
                    }

                    h3 { "Editor" }
                    label { class: "mk-settings-check",
                        input { r#type: "checkbox", checked: settings.editor.wrap,
                            onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.editor.wrap = Some(v))); } }
                        "Wrap long lines in the code editor (Alt+Z toggles)"
                    }

                    h3 { "Terminal" }
                    label { "Shell"
                        input { class: "mk-input", value: "{settings.terminal.shell.clone().unwrap_or_default()}", placeholder: "$SHELL",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.terminal.shell = opt(v))); } }
                    }

                    h3 { "Extensions" }
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

                    h3 { "You" }
                    label { "Name"
                        input { class: "mk-input", value: "{settings.user_name}", placeholder: "shown in history and presence",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.user_name = opt(v))); } }
                    }

                    h3 { "Keybindings" }
                    p { class: "mk-muted", "Ctrl also matches Cmd. Empty = unbound; a scope only stores the bindings changed there." }
                    for entry in registry.read().entries.iter() {
                        {
                            let id = entry.id.clone();
                            let bound = entry.binding.as_ref().map(|b| b.display()).unwrap_or_default();
                            let overridden = settings.keybindings.contains_key(&id);
                            rsx! {
                                label { key: "{id}", class: "mk-settings-key",
                                    span { "{entry.title}" if overridden { span { class: "mk-settings-scope", "custom" } } }
                                    input { class: "mk-input", value: "{bound}", placeholder: "unbound", "data-command": "{id}",
                                        onchange: {
                                            let id = id.clone();
                                            move |e| {
                                                let v = e.value().trim().to_string();
                                                let id = id.clone();
                                                if !v.is_empty() && moonkale_ext_api::Keybinding::parse(&v).is_none() {
                                                    ws.set_status(format!("Not a keybinding: {v} (try Ctrl+Shift+P, F12, Alt+ArrowUp)"));
                                                    return;
                                                }
                                                apply(Box::new(move |f| { f.keybindings.insert(id, v); }));
                                            }
                                        } }
                                }
                            }
                        }
                    }

                    h3 { "Remembered" }
                    p { class: "mk-muted",
                        "Recent folders: {settings.recent_folders.len()} · layout saved: {settings.layout.is_some()} · open documents: {settings.open_documents.len()}"
                    }
                }
            } else {
                div { class: "mk-settings-json",
                    h3 { "User file" span { class: "mk-settings-scope", "this machine" } }
                    pre { "{user.to_json()}" }
                    h3 { "Workspace file" span { class: "mk-settings-scope", ".moonkale/settings.json" } }
                    pre { if has_workspace { "{workspace.to_json()}" } else { "(no folder open)" } }
                    h3 { "Environment overrides" }
                    pre { "{env.to_json()}" }
                }
            }
        }
    }
}
