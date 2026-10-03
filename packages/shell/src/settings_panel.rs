//! Settings panel (Milestone 5): the resolved settings as a form, each
//! field with a scope switch (user / workspace), plus the raw JSON of both
//! files. Changes apply live and persist on change.
//!
//! Since Milestone 15 (Prompt24) the panel holds what is *not* one
//! extension's: the saved agents (language-model profiles), search, the
//! selectors that pick one extension among several for the same job, you,
//! keybindings. Everything an extension owns — its on/off switch, its
//! permissions, its own settings — is in the Extensions panel under the
//! extension.

use crate::L;
use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::settings::{AgentProfileFile, LlmFile, Scope, SettingsFile, DEFAULT_AGENT};
use moonkale_ext_api::t;

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

    fn panels(&self, ws: Workspace) -> Vec<PanelContribution> {
        vec![
            PanelContribution::new(PANEL_ID, t!(ws, L, "settings-title"), PanelHome::Main)
                .closable(true)
                .activity(Activity::new("settings", 900, t!(ws, L, "settings-title"))),
        ]
    }

    fn render(&self, _panel_id: &str, ws: Workspace) -> Element {
        rsx! { SettingsPanel { ws } }
    }
}

/// Which file a change goes to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
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

fn opt(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.trim().to_string())
    }
}

/// The provider kinds offered, in order.
pub const PROVIDERS: [&str; 5] = ["mock", "claude-code", "anthropic", "openai", "ollama"];

/// A provider kind's label in the user's language.
fn provider_label(ws: Workspace, kind: &str) -> String {
    match kind {
        "mock" => t!(ws, L, "provider-mock"),
        "claude-code" => t!(ws, L, "provider-claude-code"),
        "anthropic" => "Anthropic".into(),
        "openai" => t!(ws, L, "provider-openai"),
        "ollama" => t!(ws, L, "provider-ollama"),
        other => other.into(),
    }
}

#[component]
fn SettingsPanel(ws: Workspace) -> Element {
    let mut target = use_signal(|| Target::User);
    let mut tab = use_signal(|| "form");
    let mut secret_name = use_signal(String::new);
    let mut secret_value = use_signal(String::new);
    let mut secret_status = use_signal(String::new);

    let registry = use_context::<crate::commands::CommandRegistry>();
    let settings = ws.settings.resolved.read().clone();
    let user = ws.settings.user.read().clone();
    let workspace = ws.settings.workspace.read().clone();
    let env = moonkale_ext_api::settings::Settings::env_overrides();
    let has_workspace = ws.settings.folder.read().is_some();
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

    let embed_model = settings.llm.embed_model.clone().unwrap_or_default();
    let agents = settings.agents.clone();
    let default_agent = settings.agent.default.clone();
    let editor_impl = settings.editor.implementation.clone();
    let terminal_impl = settings.terminal.implementation.clone();
    let next_name = format!("Agent {}", agents.len());

    rsx! {
        div { class: "mk-settings",
            div { class: "mk-settings-toolbar",
                span { class: "mk-settings-tabs",
                    button { class: if tab() == "form" { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| tab.set("form"), {t!(ws, L, "settings-form")} }
                    button { class: if tab() == "json" { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| tab.set("json"), "JSON" }
                }
                span { class: "mk-settings-spacer" }
                span { class: "mk-muted", {t!(ws, L, "settings-changes-go-to")} }
                button { class: if target() == Target::User { "mk-btn mk-btn-on" } else { "mk-btn" }, onclick: move |_| target.set(Target::User), title: t!(ws, L, "settings-user-title"), {t!(ws, L, "settings-user")} }
                button { class: if target() == Target::Workspace { "mk-btn mk-btn-on" } else { "mk-btn" }, disabled: !has_workspace, onclick: move |_| target.set(Target::Workspace), title: t!(ws, L, "settings-workspace-title"), {t!(ws, L, "settings-workspace")} }
            }
            if !store_available {
                p { class: "mk-settings-note", {t!(ws, L, "settings-not-persisted")} }
            }
            if !settings.ignored_from_folder.is_empty() {
                p { class: "mk-settings-note mk-settings-ignored",
                    title: t!(ws, L, "settings-ignored-title"),
                    {t!(ws, L, "settings-ignored", fields = settings.ignored_from_folder.join(", "))}
                }
            }
            if tab() == "form" {
                div { class: "mk-settings-form",
                    h3 { {t!(ws, L, "settings-agents")} span { class: "mk-settings-scope", "{scope_label(llm_scope)}" } }
                    p { class: "mk-muted", {t!(ws, L, "settings-agents-hint")} }
                    for profile in agents.iter() {
                        AgentProfileCard { key: "{profile.name}", ws, target: target(), profile: profile.clone(), is_default: profile.name == default_agent, only_one: agents.len() == 1 }
                    }
                    div { class: "mk-settings-actions",
                        button { class: "mk-btn mk-settings-add-agent", onclick: move |_| { let name = next_name.clone(); apply(Box::new(move |f| f.agents.push(AgentProfileFile { name, llm: LlmFile { provider: Some("mock".into()), ..Default::default() } }))); }, {t!(ws, L, "settings-add-agent")} }
                    }
                    if ws.secret_store().is_some() {
                        div { class: "mk-settings-secret",
                            input { class: "mk-input", placeholder: t!(ws, L, "settings-secret-name"), value: "{secret_name}", oninput: move |e| secret_name.set(e.value()) }
                            input { class: "mk-input", r#type: "password", placeholder: t!(ws, L, "settings-api-key"), value: "{secret_value}", oninput: move |e| secret_value.set(e.value()) }
                            button { class: "mk-btn", disabled: secret_name().trim().is_empty(), onclick: move |_| {
                                let name = secret_name.peek().trim().to_string();
                                let value = secret_value.peek().clone();
                                let store = ws.secret_store().unwrap();
                                spawn(async move {
                                    match store(name.clone(), value).await {
                                        Ok(()) => { secret_status.set(t!(ws, L, "settings-secret-stored", name = format!("{name:?}"))); secret_value.set(String::new()); }
                                        Err(e) => secret_status.set(t!(ws, L, "settings-secret-not-stored", error = e)),
                                    }
                                });
                            }, {t!(ws, L, "settings-store-secret")} }
                            span { class: "mk-muted", "{secret_status}" }
                        }
                    }

                    h3 { {t!(ws, L, "search-title")} }
                    label { {t!(ws, L, "settings-embedding-model")}
                        input { class: "mk-input", value: "{embed_model}", placeholder: t!(ws, L, "settings-embedding-none"), title: t!(ws, L, "settings-embedding-title"),
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.embed_model = opt(v))); } }
                    }
                    label { class: "mk-settings-check",
                        input { r#type: "checkbox", checked: settings.search.embeddings,
                            onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.search.embeddings = Some(v))); } }
                        {t!(ws, L, "settings-use-embeddings")}
                    }

                    // Milestone 15: the one extension-related thing here — which of
                    // several extensions does a job. On/off and each extension's own
                    // settings are in the Extensions panel.
                    h3 { {t!(ws, L, "settings-which-extension")} }
                    label { {t!(ws, L, "settings-code-editor")}
                        select { class: "mk-input mk-settings-editor-impl", value: "{editor_impl}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.editor.implementation = Some(v))); },
                            option { value: "codemirror", selected: editor_impl == "codemirror", {t!(ws, L, "settings-editor-codemirror")} }
                            option { value: "native", selected: editor_impl == "native", {t!(ws, L, "settings-editor-native")} }
                        }
                    }
                    label { {t!(ws, L, "settings-terminal")}
                        select { class: "mk-input mk-settings-terminal-impl", value: "{terminal_impl}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.terminal.implementation = Some(v))); },
                            option { value: "ask", selected: terminal_impl == "ask", {t!(ws, L, "settings-terminal-ask")} }
                            option { value: "xterm", selected: terminal_impl == "xterm", {t!(ws, L, "settings-terminal-xterm")} }
                            option { value: "native", selected: terminal_impl == "native", {t!(ws, L, "settings-terminal-native")} }
                        }
                    }
                    p { class: "mk-muted", {t!(ws, L, "settings-which-hint")} }

                    h3 { {t!(ws, L, "settings-you")} }
                    label { {t!(ws, L, "settings-name")}
                        input { class: "mk-input", value: "{settings.user_name}", placeholder: t!(ws, L, "settings-name-placeholder"),
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.user_name = opt(v))); } }
                    }

                    // Spec 030: the UI language and the theme.
                    h3 { {t!(ws, L, "settings-appearance")} }
                    label { {t!(ws, L, "settings-language")}
                        select { class: "mk-input mk-settings-language", value: "{settings.language}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.language = opt(v))); },
                            option { value: "", selected: settings.language.is_empty(), {t!(ws, L, "settings-language-system", lang = ws.settings.system_language.read().clone())} }
                            for (tag, name) in moonkale_ext_api::i18n::LANGUAGES {
                                option { value: "{tag}", selected: settings.language == *tag, "{name}" }
                            }
                        }
                    }
                    label { {t!(ws, L, "settings-theme")}
                        select { class: "mk-input mk-settings-theme", value: "{settings.theme}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.theme = opt(v))); },
                            option { value: "dark", selected: settings.theme == "dark", {t!(ws, L, "settings-theme-dark")} }
                            option { value: "light", selected: settings.theme == "light", {t!(ws, L, "settings-theme-light")} }
                            option { value: "system", selected: settings.theme == "system", {t!(ws, L, "settings-theme-system")} }
                            for name in crate::theme::extra_themes(ws) {
                                option { value: "{name}", selected: settings.theme == name, "{name}" }
                            }
                        }
                    }

                    h3 { {t!(ws, L, "settings-keybindings")} }
                    p { class: "mk-muted", {t!(ws, L, "settings-keybindings-hint")} }
                    for entry in registry.read().entries.iter() {
                        {
                            let id = entry.id.clone();
                            let bound = entry.binding.as_ref().map(|b| b.display()).unwrap_or_default();
                            let overridden = settings.keybindings.contains_key(&id);
                            rsx! {
                                label { key: "{id}", class: "mk-settings-key",
                                    span { "{entry.title}" if overridden { span { class: "mk-settings-scope", {t!(ws, L, "settings-custom")} } } }
                                    input { class: "mk-input", value: "{bound}", placeholder: t!(ws, L, "settings-unbound"), "data-command": "{id}",
                                        onchange: {
                                            let id = id.clone();
                                            move |e| {
                                                let v = e.value().trim().to_string();
                                                let id = id.clone();
                                                if !v.is_empty() && moonkale_ext_api::Keybinding::parse(&v).is_none() {
                                                    ws.set_status(t!(ws, L, "settings-not-a-keybinding", value = v));
                                                    return;
                                                }
                                                apply(Box::new(move |f| { f.keybindings.insert(id, v); }));
                                            }
                                        } }
                                }
                            }
                        }
                    }

                    h3 { {t!(ws, L, "settings-remembered")} }
                    p { class: "mk-muted",
                        {t!(ws, L, "settings-remembered-line", recent = settings.recent_folders.len(), connections = settings.remote_saved.len(), layout = if settings.layout.is_some() { "yes" } else { "no" }, open = settings.open_documents.len())}
                    }
                }
            } else {
                div { class: "mk-settings-json",
                    h3 { {t!(ws, L, "settings-json-user")} span { class: "mk-settings-scope", {t!(ws, L, "settings-this-machine")} } }
                    pre { "{user.to_json()}" }
                    h3 { {t!(ws, L, "settings-json-workspace")} span { class: "mk-settings-scope", ".moonkale/settings.json" } }
                    pre { if has_workspace { "{workspace.to_json()}" } else { {t!(ws, L, "settings-no-folder")} } }
                    h3 { {t!(ws, L, "settings-json-env")} }
                    pre { "{env.to_json()}" }
                }
            }
        }
    }
}

/// Edit one profile's `LlmFile` in the target file: the flat `llm` for
/// Default, else the entry of that name (created when the scope has none).
fn edit_profile(f: &mut SettingsFile, name: &str, edit: impl FnOnce(&mut LlmFile)) {
    if name == DEFAULT_AGENT {
        edit(&mut f.llm);
        return;
    }
    match f.agents.iter_mut().find(|a| a.name == name) {
        Some(a) => edit(&mut a.llm),
        None => {
            let mut a = AgentProfileFile {
                name: name.to_string(),
                llm: LlmFile::default(),
            };
            edit(&mut a.llm);
            f.agents.push(a);
        }
    }
}

/// One saved agent (Milestone 15): name, provider, model, endpoint or
/// command, secret, the Claude Code knobs and its login status.
#[component]
fn AgentProfileCard(
    ws: Workspace,
    target: Target,
    profile: moonkale_ext_api::settings::AgentProfile,
    is_default: bool,
    only_one: bool,
) -> Element {
    let name = profile.name.clone();
    let llm = profile.llm.clone();
    let provider = llm.provider.clone();
    let is_default_profile = name == DEFAULT_AGENT;
    let apply = move |f: Box<dyn FnOnce(&mut SettingsFile)>| {
        spawn(async move {
            match target {
                Target::User => ws.update_user_settings(|file| f(file)).await,
                Target::Workspace => ws.update_workspace_settings(|file| f(file)).await,
            }
        });
    };
    let edit = {
        let name = name.clone();
        move |g: Box<dyn FnOnce(&mut LlmFile)>| {
            let name = name.clone();
            apply(Box::new(move |f| edit_profile(f, &name, g)));
        }
    };
    let n1 = name.clone();
    let n2 = name.clone();
    let n3 = name.clone();
    let permission_mode = llm
        .options
        .get("permission_mode")
        .cloned()
        .unwrap_or_else(|| "plan".into());
    let allowed_tools = llm
        .options
        .get("allowed_tools")
        .cloned()
        .unwrap_or_default();
    rsx! {
        div { class: if is_default { "mk-settings-agent mk-settings-agent-default" } else { "mk-settings-agent" }, "data-agent": "{name}",
            div { class: "mk-settings-agent-head",
                if is_default_profile {
                    span { class: "mk-settings-agent-name", "{name}" }
                } else {
                    input { class: "mk-input mk-settings-agent-name", value: "{name}", title: t!(ws, L, "settings-rename-agent"),
                        onchange: move |e| {
                            let new = e.value().trim().to_string();
                            let old = n1.clone();
                            if new.is_empty() || new == old || new == DEFAULT_AGENT { return; }
                            apply(Box::new(move |f| {
                                match f.agents.iter_mut().find(|a| a.name == old) {
                                    Some(a) => a.name = new.clone(),
                                    None => f.agents.push(AgentProfileFile { name: new.clone(), llm: LlmFile::default() }),
                                }
                                if f.agent.default.as_deref() == Some(old.as_str()) { f.agent.default = Some(new); }
                            }));
                        } }
                }
                label { class: "mk-settings-check mk-settings-agent-runs",
                    input { r#type: "radio", name: "mk-default-agent", checked: is_default,
                        onchange: move |_| { let n = n2.clone(); apply(Box::new(move |f| f.agent.default = if n == DEFAULT_AGENT { None } else { Some(n) })); } }
                    {t!(ws, L, "settings-runs-by-default")}
                }
                span { class: "mk-settings-spacer" }
                if !is_default_profile {
                    button { class: "mk-btn mk-settings-agent-remove", disabled: only_one, title: t!(ws, L, "settings-forget-agent"),
                        onclick: move |_| { let n = n3.clone(); apply(Box::new(move |f| { f.agents.retain(|a| a.name != n); if f.agent.default.as_deref() == Some(n.as_str()) { f.agent.default = None; } })); }, {t!(ws, L, "settings-remove")} }
                }
            }
            label { {t!(ws, L, "settings-provider")}
                select { class: "mk-input mk-settings-agent-provider", value: "{provider}",
                    onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.provider = Some(v))); } },
                    for k in PROVIDERS {
                        option { value: "{k}", selected: provider == k, {provider_label(ws, k)} }
                    }
                }
            }
            label { {t!(ws, L, "settings-model")}
                input { class: "mk-input mk-settings-agent-model", value: "{llm.model}", placeholder: if provider == "claude-code" { t!(ws, L, "settings-model-claude") } else { t!(ws, L, "settings-model-default") },
                    onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.model = opt(v))); } } }
            }
            if provider == "claude-code" {
                // Milestone 12: the CLI runs its own tools in the open folder; these are its knobs.
                label { {t!(ws, L, "settings-command")}
                    input { class: "mk-input", value: "{llm.base_url}", placeholder: t!(ws, L, "settings-command-placeholder"),
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.base_url = opt(v))); } } }
                }
                label { {t!(ws, L, "settings-permissions")}
                    select { class: "mk-input", value: "{permission_mode}",
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| { l.options.insert("permission_mode".into(), v); })); } },
                        for (k, label) in [("plan", t!(ws, L, "settings-mode-plan")), ("default", t!(ws, L, "settings-mode-default")), ("acceptEdits", t!(ws, L, "settings-mode-accept")), ("bypassPermissions", t!(ws, L, "settings-mode-bypass"))] {
                            option { value: "{k}", selected: permission_mode == k, "{label}" }
                        }
                    }
                }
                label { {t!(ws, L, "settings-allowed-tools")}
                    input { class: "mk-input", value: "{allowed_tools}", placeholder: t!(ws, L, "settings-allowed-tools-placeholder"),
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| { l.options.insert("allowed_tools".into(), v); })); } } }
                }
                ClaudeStatus { ws, llm: llm.clone() }
            } else {
                label { {t!(ws, L, "settings-endpoint")}
                    input { class: "mk-input", value: "{llm.base_url}", placeholder: "https://api.openai.com/v1 · https://api.mistral.ai/v1 · http://127.0.0.1:11434",
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.base_url = opt(v))); } } }
                }
                label { {t!(ws, L, "settings-secret")}
                    input { class: "mk-input", value: "{llm.secret}", placeholder: t!(ws, L, "settings-secret-placeholder"),
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.secret = opt(v))); } } }
                }
            }
        }
    }
}

/// The Claude Code CLI's state for one profile (Milestone 15): version and
/// login from `Provider::status`, a *Log in* button that runs `claude auth
/// login` in a terminal tab (the CLI opens the browser and asks for the
/// code there), and *Check again*.
#[component]
fn ClaudeStatus(
    ws: Workspace,
    llm: ReadSignal<moonkale_ext_api::settings::LlmSettings>,
) -> Element {
    let mut generation = use_signal(|| 0u32);
    let status = use_resource(move || {
        let _ = generation();
        let llm = llm();
        async move {
            let make = ws.llm()?;
            let p = make(llm).await.ok()?;
            p.status().await
        }
    });
    let command = {
        let base = llm.read().base_url.trim().to_string();
        if base.is_empty() {
            "claude".to_string()
        } else {
            base
        }
    };
    let can_run = ws.can_run_program();
    let login = move |_| {
        let command = command.clone();
        spawn(async move {
            ws.run_in_terminal(
                "claude login",
                &command,
                vec!["auth".into(), "login".into()],
            )
            .await;
            let mut ws = ws;
            ws.set_status(t!(ws, L, "claude-login-status"));
        });
    };
    rsx! {
        div { class: "mk-settings-claude",
            match status() {
                None => rsx! { span { class: "mk-muted mk-settings-claude-status", {t!(ws, L, "claude-checking")} } },
                Some(None) => rsx! { span { class: "mk-muted mk-settings-claude-status", {t!(ws, L, "claude-no-status")} } },
                Some(Some(st)) => rsx! {
                    span { class: if st.ok { "mk-settings-claude-status mk-settings-ok" } else { "mk-settings-claude-status mk-settings-bad" }, "{st.summary}" }
                    if let Some(h) = st.hint.clone() { span { class: "mk-muted", " {h}" } }
                    if st.can_login && can_run {
                        button { class: "mk-btn mk-settings-claude-login", onclick: login, title: t!(ws, L, "claude-login-title"), if st.ok { {t!(ws, L, "claude-login-again")} } else { {t!(ws, L, "claude-login")} } }
                    }
                },
            }
            button { class: "mk-btn", onclick: move |_| generation += 1, {t!(ws, L, "claude-check-again")} }
            p { class: "mk-muted", {t!(ws, L, "claude-hint")} }
        }
    }
}
