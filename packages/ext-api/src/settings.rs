//! Settings: what Moonkale remembers between launches (Milestone 5).
//!
//! Two persisted scopes, one resolved value:
//!
//! ```text
//!   Settings::default()  ←  user file  ←  workspace file  ←  env overrides
//!   (built-in)              (per machine)  (.moonkale/settings.json)
//! ```
//!
//! [`SettingsFile`] is the persisted shape: every field optional, so a
//! scope only overrides what it sets; unknown fields are ignored and a
//! `version` field guards the format. [`Settings`] is the resolved shape
//! the app reads. Secrets are never in either: providers name a
//! [`SecretRef`] that the platform resolves (keychain, environment, secrets
//! file) — see `WorkspaceConfig::secret`.
//!
//! Scope-specific data lives in the same files: `recent_folders` is user
//! data, `layout` / `open_documents` are workspace data. Both are
//! [`SettingsFile`] fields so one loader serves both.

use serde::{Deserialize, Serialize};

pub const SETTINGS_VERSION: u32 = 1;
/// Workspace settings path, relative to the folder root.
pub const WORKSPACE_FILE: &str = ".moonkale/settings.json";

/// Name of a secret the platform resolves (never the secret itself).
pub type SecretRef = String;

/// The persisted shape (either scope). All optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsFile {
    pub version: u32,
    pub llm: LlmFile,
    pub policy: PolicyFile,
    pub search: SearchFile,
    pub terminal: TerminalFile,
    pub extensions: ExtensionsFile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// User scope: most recent first, absolute paths (or server-relative on web).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_folders: Vec<String>,
    /// Workspace scope: `PanelLayout::encode()` output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Workspace scope: relative keys of the documents that were open.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open_documents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_document: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmFile {
    /// `anthropic` | `openai` | `ollama` | `mock`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// OpenAI-compatible endpoint or Ollama host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_model: Option<String>,
    /// Which secret holds the API key (default: the provider's name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyFile {
    /// Let the agent run mutating tools without asking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_writes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_tools: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<bool>,
}

/// Which optional extensions are on: an explicit list per state; anything
/// unlisted follows the manifest's `default_enabled`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtensionsFile {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enabled: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<String>,
    /// Granted permissions per extension id (`"read-sources"`, …).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub permissions: std::collections::BTreeMap<String, Vec<String>>,
}

impl ExtensionsFile {
    /// Record a choice (removing it from the opposite list).
    pub fn set_enabled(&mut self, id: &str, on: bool) {
        self.enabled.retain(|e| e != id);
        self.disabled.retain(|e| e != id);
        if on {
            self.enabled.push(id.to_string());
        } else {
            self.disabled.push(id.to_string());
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

impl SettingsFile {
    pub fn new() -> Self {
        Self {
            version: SETTINGS_VERSION,
            ..Default::default()
        }
    }

    /// Tolerant decode: unknown fields ignored, a newer major version refused.
    pub fn parse(json: &str) -> Result<Self, String> {
        let file: SettingsFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if file.version > SETTINGS_VERSION {
            return Err(format!(
                "settings version {} is newer than this Moonkale ({SETTINGS_VERSION})",
                file.version
            ));
        }
        Ok(file)
    }

    pub fn to_json(&self) -> String {
        let mut f = self.clone();
        f.version = SETTINGS_VERSION;
        serde_json::to_string_pretty(&f).unwrap_or_else(|_| "{}".into())
    }

    /// Overlay `other` on `self`: set fields win, lists replace.
    pub fn overlay(&mut self, other: &SettingsFile) {
        let o = &other.llm;
        let l = &mut self.llm;
        l.provider = o.provider.clone().or(l.provider.take());
        l.model = o.model.clone().or(l.model.take());
        l.base_url = o.base_url.clone().or(l.base_url.take());
        l.embed_model = o.embed_model.clone().or(l.embed_model.take());
        l.secret = o.secret.clone().or(l.secret.take());
        self.policy.allow_writes = other.policy.allow_writes.or(self.policy.allow_writes);
        self.policy.denied_tools = other
            .policy
            .denied_tools
            .clone()
            .or(self.policy.denied_tools.take());
        self.search.embeddings = other.search.embeddings.or(self.search.embeddings);
        self.terminal.shell = other.terminal.shell.clone().or(self.terminal.shell.take());
        // Extensions: a later scope's explicit choice wins per id.
        for id in &other.extensions.enabled {
            self.extensions.set_enabled(id, true);
        }
        for id in &other.extensions.disabled {
            self.extensions.set_enabled(id, false);
        }
        for (id, perms) in &other.extensions.permissions {
            self.extensions
                .permissions
                .insert(id.clone(), perms.clone());
        }
        self.theme = other.theme.clone().or(self.theme.take());
        if !other.recent_folders.is_empty() {
            self.recent_folders = other.recent_folders.clone();
        }
        self.layout = other.layout.clone().or(self.layout.take());
        if !other.open_documents.is_empty() {
            self.open_documents = other.open_documents.clone();
        }
        self.active_document = other
            .active_document
            .clone()
            .or(self.active_document.take());
    }

    /// Remember a folder (most recent first, at most 12).
    pub fn push_recent(&mut self, path: &str) {
        self.recent_folders.retain(|p| p != path);
        self.recent_folders.insert(0, path.to_string());
        self.recent_folders.truncate(12);
    }
}

/// Where a resolved value came from (shown as a badge in the Settings panel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Default,
    User,
    Workspace,
    Env,
}

/// The resolved settings the app reads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub llm: LlmSettings,
    pub policy: PolicySettings,
    pub search: SearchSettings,
    pub terminal: TerminalSettings,
    pub extensions: ExtensionsSettings,
    pub theme: String,
    pub recent_folders: Vec<String>,
    pub layout: Option<String>,
    pub open_documents: Vec<String>,
    pub active_document: Option<String>,
}

pub use moonkale_llm::LlmSettings;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolicySettings {
    pub allow_writes: bool,
    pub denied_tools: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchSettings {
    pub embeddings: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TerminalSettings {
    pub shell: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtensionsSettings {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
    pub permissions: std::collections::BTreeMap<String, Vec<String>>,
}

impl ExtensionsSettings {
    /// Is this extension on? Core ones always; optional ones per the explicit
    /// lists, else the manifest default.
    pub fn is_enabled(&self, m: &crate::Manifest) -> bool {
        if !m.optional {
            return true;
        }
        if self.disabled.iter().any(|d| d == m.id) {
            return false;
        }
        if self.enabled.iter().any(|e| e == m.id) {
            return true;
        }
        m.default_enabled
    }

    /// Third-party (wasm) extensions: enabled only when listed, default off.
    pub fn is_enabled_id(&self, id: &str, default: bool) -> bool {
        if self.disabled.iter().any(|d| d == id) {
            return false;
        }
        if self.enabled.iter().any(|e| e == id) {
            return true;
        }
        default
    }

    /// Permissions granted to `id` (built-in extensions get what they declare
    /// unless the user removed some).
    pub fn granted(&self, m: &crate::Manifest) -> Vec<String> {
        match self.permissions.get(m.id) {
            Some(p) => p.clone(),
            None => m.permissions.iter().map(|p| p.to_string()).collect(),
        }
    }

    pub fn has(&self, m: &crate::Manifest, permission: &str) -> bool {
        self.granted(m).iter().any(|p| p == permission)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            llm: LlmSettings {
                provider: "mock".into(),
                model: String::new(),
                base_url: String::new(),
                embed_model: None,
                secret: String::new(),
            },
            policy: PolicySettings::default(),
            search: SearchSettings { embeddings: true },
            terminal: TerminalSettings::default(),
            extensions: ExtensionsSettings::default(),
            theme: "dark".into(),
            recent_folders: Vec::new(),
            layout: None,
            open_documents: Vec::new(),
            active_document: None,
        }
    }
}

impl Settings {
    /// Resolve: defaults ← user ← workspace ← env.
    pub fn resolve(user: &SettingsFile, workspace: &SettingsFile, env: &SettingsFile) -> Self {
        let mut merged = SettingsFile::new();
        merged.overlay(user);
        merged.overlay(workspace);
        merged.overlay(env);
        let d = Settings::default();
        let provider = merged.llm.provider.unwrap_or(d.llm.provider);
        Self {
            llm: LlmSettings {
                secret: merged.llm.secret.unwrap_or_else(|| provider.clone()),
                model: merged.llm.model.unwrap_or_default(),
                base_url: merged.llm.base_url.unwrap_or_default(),
                embed_model: merged.llm.embed_model,
                provider,
            },
            policy: PolicySettings {
                allow_writes: merged.policy.allow_writes.unwrap_or(false),
                denied_tools: merged.policy.denied_tools.unwrap_or_default(),
            },
            search: SearchSettings {
                embeddings: merged.search.embeddings.unwrap_or(true),
            },
            terminal: TerminalSettings {
                shell: merged.terminal.shell,
            },
            extensions: ExtensionsSettings {
                enabled: merged.extensions.enabled,
                disabled: merged.extensions.disabled,
                permissions: merged.extensions.permissions,
            },
            theme: merged.theme.unwrap_or(d.theme),
            recent_folders: user.recent_folders.clone(),
            layout: workspace.layout.clone(),
            open_documents: workspace.open_documents.clone(),
            active_document: workspace.active_document.clone(),
        }
    }

    /// Which scope decided `llm.provider` (and by extension the model).
    pub fn scope_of_llm(
        user: &SettingsFile,
        workspace: &SettingsFile,
        env: &SettingsFile,
    ) -> Scope {
        if env.llm.provider.is_some() {
            Scope::Env
        } else if workspace.llm.provider.is_some() {
            Scope::Workspace
        } else if user.llm.provider.is_some() {
            Scope::User
        } else {
            Scope::Default
        }
    }

    /// Environment overrides (native only): the Milestone 4 variables keep
    /// working and win over the files.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn env_overrides() -> SettingsFile {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let mut f = SettingsFile::new();
        f.llm.provider = var("MOONKALE_LLM").or_else(|| {
            if var("ANTHROPIC_API_KEY").is_some() {
                Some("anthropic".into())
            } else if var("OPENAI_API_KEY").is_some() {
                Some("openai".into())
            } else if var("OLLAMA_HOST").is_some() {
                Some("ollama".into())
            } else {
                None
            }
        });
        f.llm.model = var("MOONKALE_LLM_MODEL");
        f.llm.base_url = var("OPENAI_BASE_URL").or_else(|| var("OLLAMA_HOST"));
        f.llm.embed_model = var("MOONKALE_EMBED_MODEL");
        f.terminal.shell = var("MOONKALE_SHELL");
        f
    }
    #[cfg(target_arch = "wasm32")]
    pub fn env_overrides() -> SettingsFile {
        SettingsFile::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_order_and_scope() {
        let mut user = SettingsFile::new();
        user.llm.provider = Some("openai".into());
        user.llm.model = Some("gpt-4o-mini".into());
        user.push_recent("/a");
        user.push_recent("/b");
        let mut ws = SettingsFile::new();
        ws.llm.model = Some("codestral-latest".into());
        ws.policy.allow_writes = Some(true);
        ws.layout = Some("{...}".into());
        let env = SettingsFile::new();
        let s = Settings::resolve(&user, &ws, &env);
        assert_eq!(s.llm.provider, "openai");
        assert_eq!(s.llm.model, "codestral-latest");
        assert_eq!(s.llm.secret, "openai");
        assert!(s.policy.allow_writes);
        assert_eq!(s.recent_folders, ["/b", "/a"]);
        assert_eq!(s.layout.as_deref(), Some("{...}"));
        assert_eq!(Settings::scope_of_llm(&user, &ws, &env), Scope::User);
        let mut env = SettingsFile::new();
        env.llm.provider = Some("mock".into());
        assert_eq!(Settings::resolve(&user, &ws, &env).llm.provider, "mock");
        assert_eq!(Settings::scope_of_llm(&user, &ws, &env), Scope::Env);
    }

    #[test]
    fn extension_enablement() {
        let core = crate::Manifest::core("a", "A", "");
        let opt = crate::Manifest::optional("b", "B", "");
        let opt_in = crate::Manifest::opt_in("c", "C", "").with_permissions(&["network"]);
        let mut user = SettingsFile::new();
        user.extensions.set_enabled("c", true);
        user.extensions.set_enabled("b", false);
        let mut ws = SettingsFile::new();
        ws.extensions.set_enabled("b", true); // workspace re-enables B
        let s = Settings::resolve(&user, &ws, &SettingsFile::new());
        assert!(s.extensions.is_enabled(&core));
        assert!(s.extensions.is_enabled(&opt));
        assert!(s.extensions.is_enabled(&opt_in));
        assert!(!Settings::resolve(
            &SettingsFile::new(),
            &SettingsFile::new(),
            &SettingsFile::new()
        )
        .extensions
        .is_enabled(&opt_in));
        assert!(s.extensions.has(&opt_in, "network"));
        assert!(!s.extensions.has(&opt, "network"));
    }

    #[test]
    fn tolerant_parse_and_round_trip() {
        let f = SettingsFile::parse(
            r#"{"version":1,"llm":{"provider":"ollama","future_field":1},"unknown":{}}"#,
        )
        .unwrap();
        assert_eq!(f.llm.provider.as_deref(), Some("ollama"));
        assert!(SettingsFile::parse(r#"{"version":99}"#).is_err());
        let json = f.to_json();
        assert_eq!(SettingsFile::parse(&json).unwrap(), f);
        assert!(
            !json.contains("recent_folders"),
            "empty lists are omitted: {json}"
        );
    }
}
