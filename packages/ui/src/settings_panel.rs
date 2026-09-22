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

use dioxus::prelude::*;
use moonkale_ext_api::prelude::*;
use moonkale_ext_api::settings::{AgentProfileFile, LlmFile, Scope, SettingsFile, DEFAULT_AGENT};

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
pub const PROVIDERS: [(&str, &str); 5] = [
    ("mock", "mock (offline)"),
    ("claude-code", "Claude Code (subscription, no API key)"),
    ("anthropic", "Anthropic"),
    ("openai", "OpenAI-compatible (OpenAI, Mistral, …)"),
    ("ollama", "Ollama (local)"),
];

#[component]
fn SettingsPanel(ws: Workspace) -> Element {
    let mut target = use_signal(|| Target::User);
    let mut tab = use_signal(|| "form");
    let mut secret_name = use_signal(String::new);
    let mut secret_value = use_signal(String::new);
    let mut secret_status = use_signal(String::new);

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
                    h3 { "Agents" span { class: "mk-settings-scope", "{scope_label(llm_scope)}" } }
                    p { class: "mk-muted", "Saved agents: a name and a language model each. One runs by default; every session in the Agent panel can pick another. Keys are never stored in settings — they come from MOONKALE_SECRET_<NAME>, ANTHROPIC_API_KEY / OPENAI_API_KEY, or the secrets file (desktop) / the server's (web)." }
                    for profile in agents.iter() {
                        AgentProfileCard { key: "{profile.name}", ws, target: target(), profile: profile.clone(), is_default: profile.name == default_agent, only_one: agents.len() == 1 }
                    }
                    div { class: "mk-settings-actions",
                        button { class: "mk-btn mk-settings-add-agent", onclick: move |_| { let name = next_name.clone(); apply(Box::new(move |f| f.agents.push(AgentProfileFile { name, llm: LlmFile { provider: Some("mock".into()), ..Default::default() } }))); }, "Add agent" }
                    }
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

                    h3 { "Search" }
                    label { "Embedding model"
                        input { class: "mk-input", value: "{embed_model}", placeholder: "none (keyword search only)", title: "Served by the Default agent's provider",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.llm.embed_model = opt(v))); } }
                    }
                    label { class: "mk-settings-check",
                        input { r#type: "checkbox", checked: settings.search.embeddings,
                            onchange: move |e| { let v = e.checked(); apply(Box::new(move |f| f.search.embeddings = Some(v))); } }
                        "Use embeddings when an embedding model is configured (applies to folders opened afterwards)"
                    }

                    // Milestone 15: the one extension-related thing here — which of
                    // several extensions does a job. On/off and each extension's own
                    // settings are in the Extensions panel.
                    h3 { "Which extension" }
                    label { "Code editor"
                        select { class: "mk-input mk-settings-editor-impl", value: "{editor_impl}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.editor.implementation = Some(v))); },
                            option { value: "codemirror", selected: editor_impl == "codemirror", "CodeMirror (JavaScript; language servers, wiki-links, wrap)" }
                            option { value: "native", selected: editor_impl == "native", "Rust (dioxus-code-editor; tree-sitter for every core language, no LSP yet)" }
                        }
                    }
                    label { "Terminal"
                        select { class: "mk-input mk-settings-terminal-impl", value: "{terminal_impl}",
                            onchange: move |e| { let v = e.value(); apply(Box::new(move |f| f.terminal.implementation = Some(v))); },
                            option { value: "ask", selected: terminal_impl == "ask", "ask each time both are enabled" }
                            option { value: "xterm", selected: terminal_impl == "xterm", "xterm.js (JavaScript)" }
                            option { value: "native", selected: terminal_impl == "native", "Rust (vt100 grid, Dioxus rows)" }
                        }
                    }
                    p { class: "mk-muted", "Only extensions that are switched on count; a choice that is off falls back to the other. Switch extensions on and off in the Extensions panel (puzzle icon), where each extension's own settings are too." }

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
                        "Recent folders: {settings.recent_folders.len()} · saved connections: {settings.remote_saved.len()} · layout saved: {settings.layout.is_some()} · open documents: {settings.open_documents.len()}"
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
                    input { class: "mk-input mk-settings-agent-name", value: "{name}", title: "Rename this agent",
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
                    "runs by default"
                }
                span { class: "mk-settings-spacer" }
                if !is_default_profile {
                    button { class: "mk-btn mk-settings-agent-remove", disabled: only_one, title: "Forget this agent",
                        onclick: move |_| { let n = n3.clone(); apply(Box::new(move |f| { f.agents.retain(|a| a.name != n); if f.agent.default.as_deref() == Some(n.as_str()) { f.agent.default = None; } })); }, "Remove" }
                }
            }
            label { "Provider"
                select { class: "mk-input mk-settings-agent-provider", value: "{provider}",
                    onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.provider = Some(v))); } },
                    for (k, label) in PROVIDERS {
                        option { value: "{k}", selected: provider == k, "{label}" }
                    }
                }
            }
            label { "Model"
                input { class: "mk-input mk-settings-agent-model", value: "{llm.model}", placeholder: if provider == "claude-code" { "the subscription's default (or e.g. claude-sonnet-5)" } else { "provider default" },
                    onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.model = opt(v))); } } }
            }
            if provider == "claude-code" {
                // Milestone 12: the CLI runs its own tools in the open folder; these are its knobs.
                label { "Command"
                    input { class: "mk-input", value: "{llm.base_url}", placeholder: "claude (on PATH)",
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.base_url = opt(v))); } } }
                }
                label { "Permissions"
                    select { class: "mk-input", value: "{permission_mode}",
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| { l.options.insert("permission_mode".into(), v); })); } },
                        for (k, label) in [("plan", "plan — read and propose only"), ("default", "default — Claude Code's own rules (.claude/settings.json)"), ("acceptEdits", "acceptEdits — may edit files in the folder"), ("bypassPermissions", "bypassPermissions — everything (careful)")] {
                            option { value: "{k}", selected: permission_mode == k, "{label}" }
                        }
                    }
                }
                label { "Allowed tools"
                    input { class: "mk-input", value: "{allowed_tools}", placeholder: "e.g. Read,Grep,Bash(git:*)",
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| { l.options.insert("allowed_tools".into(), v); })); } } }
                }
                ClaudeStatus { ws, llm: llm.clone() }
            } else {
                label { "Endpoint"
                    input { class: "mk-input", value: "{llm.base_url}", placeholder: "https://api.openai.com/v1 · https://api.mistral.ai/v1 · http://127.0.0.1:11434",
                        onchange: { let edit = edit.clone(); move |e| { let v = e.value(); edit(Box::new(move |l| l.base_url = opt(v))); } } }
                }
                label { "Secret name"
                    input { class: "mk-input", value: "{llm.secret}", placeholder: "defaults to the provider name",
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
            ws.set_status("Claude Code: sign in in the browser, then paste the code into the terminal tab; press Check again afterwards");
        });
    };
    rsx! {
        div { class: "mk-settings-claude",
            match status() {
                None => rsx! { span { class: "mk-muted mk-settings-claude-status", "checking the claude CLI…" } },
                Some(None) => rsx! { span { class: "mk-muted mk-settings-claude-status", "status not available on this platform" } },
                Some(Some(st)) => rsx! {
                    span { class: if st.ok { "mk-settings-claude-status mk-settings-ok" } else { "mk-settings-claude-status mk-settings-bad" }, "{st.summary}" }
                    if let Some(h) = st.hint.clone() { span { class: "mk-muted", " {h}" } }
                    if st.can_login && can_run {
                        button { class: "mk-btn mk-settings-claude-login", onclick: login, title: "Runs `claude auth login` in a terminal tab", if st.ok { "Log in again" } else { "Log in" } }
                    }
                },
            }
            button { class: "mk-btn", onclick: move |_| generation += 1, "Check again" }
            p { class: "mk-muted", "Runs the claude CLI headless in the open folder with your subscription login; no key, nothing stored by Moonkale. Its tool calls appear in the transcript as ▸ lines. On the web the CLI runs — and is logged in — on the server." }
        }
    }
}
