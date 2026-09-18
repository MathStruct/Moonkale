//! Provider configuration from the environment. Keys are read where the
//! provider runs (desktop process, or the server behind `/api/llm`); the
//! web client never sees them.
//!
//! | variable | meaning |
//! |---|---|
//! | `MOONKALE_LLM` | `anthropic` \| `openai` \| `ollama` \| `mock` (default: first whose key/host is set, else `mock`) |
//! | `ANTHROPIC_API_KEY` | Anthropic Messages API |
//! | `OPENAI_API_KEY`, `OPENAI_BASE_URL` | any OpenAI-compatible endpoint (default `https://api.openai.com/v1`) |
//! | `OLLAMA_HOST` | Ollama server (default `http://127.0.0.1:11434`) |
//! | `MOONKALE_LLM_MODEL` | chat model (defaults per provider) |
//! | `MOONKALE_EMBED_MODEL` | embedding model (e.g. `text-embedding-3-small`, `nomic-embed-text`); unset = BM25-only search |

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    Anthropic { api_key: String },
    OpenAi { base_url: String, api_key: String },
    Ollama { host: String },
    Mock,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub kind: ProviderKind,
    pub model: String,
    pub embed_model: Option<String>,
}

impl Config {
    pub fn mock() -> Self {
        Self {
            kind: ProviderKind::Mock,
            model: "mock".into(),
            embed_model: None,
        }
    }

    /// Native only (reads the process environment).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let anthropic = var("ANTHROPIC_API_KEY");
        let openai = var("OPENAI_API_KEY");
        let ollama = var("OLLAMA_HOST");
        let choice = var("MOONKALE_LLM").unwrap_or_else(|| {
            if anthropic.is_some() {
                "anthropic".into()
            } else if openai.is_some() {
                "openai".into()
            } else if ollama.is_some() {
                "ollama".into()
            } else {
                "mock".into()
            }
        });
        let (kind, default_model) = match choice.as_str() {
            "anthropic" => (
                ProviderKind::Anthropic {
                    api_key: anthropic.unwrap_or_default(),
                },
                "claude-sonnet-5",
            ),
            "openai" => (
                ProviderKind::OpenAi {
                    base_url: var("OPENAI_BASE_URL")
                        .unwrap_or_else(|| "https://api.openai.com/v1".into()),
                    api_key: openai.unwrap_or_default(),
                },
                "gpt-4o-mini",
            ),
            "ollama" => (
                ProviderKind::Ollama {
                    host: ollama.unwrap_or_else(|| "http://127.0.0.1:11434".into()),
                },
                "qwen2.5:1.5b",
            ),
            _ => (ProviderKind::Mock, "mock"),
        };
        // Embeddings are opt-in: unset means BM25-only search (the mock
        // always embeds so tests cover the vector path).
        let embed_model = var("MOONKALE_EMBED_MODEL").or(match kind {
            ProviderKind::Mock => Some("mock-embed".into()),
            _ => None,
        });
        Self {
            model: var("MOONKALE_LLM_MODEL").unwrap_or_else(|| default_model.into()),
            embed_model,
            kind,
        }
    }

    /// Build from resolved settings (Milestone 5): the provider kind by name,
    /// its endpoint, the model (empty = provider default) and the resolved
    /// secret. Unknown kinds fall back to the mock.
    pub fn from_parts(
        provider: &str,
        model: &str,
        base_url: &str,
        embed_model: Option<String>,
        key: Option<String>,
    ) -> Self {
        let (kind, default_model) = match provider {
            "anthropic" => (
                ProviderKind::Anthropic {
                    api_key: key.unwrap_or_default(),
                },
                "claude-sonnet-5",
            ),
            "openai" => (
                ProviderKind::OpenAi {
                    base_url: if base_url.is_empty() {
                        "https://api.openai.com/v1".into()
                    } else {
                        base_url.into()
                    },
                    api_key: key.unwrap_or_default(),
                },
                "gpt-4o-mini",
            ),
            "ollama" => (
                ProviderKind::Ollama {
                    host: if base_url.is_empty() {
                        "http://127.0.0.1:11434".into()
                    } else {
                        base_url.into()
                    },
                },
                "qwen2.5:1.5b",
            ),
            _ => (ProviderKind::Mock, "mock"),
        };
        Self {
            model: if model.is_empty() {
                default_model.into()
            } else {
                model.into()
            },
            embed_model: embed_model.or(match kind {
                ProviderKind::Mock => Some("mock-embed".into()),
                _ => None,
            }),
            kind,
        }
    }

    /// [`from_parts`](Self::from_parts) for an [`LlmSettings`](crate::LlmSettings).
    pub fn from_settings(s: &crate::LlmSettings, key: Option<String>) -> Self {
        Self::from_parts(
            &s.provider,
            &s.model,
            &s.base_url,
            s.embed_model.clone(),
            key,
        )
    }

    /// Short human description for the status bar ("anthropic · claude-sonnet-5").
    pub fn label(&self) -> String {
        let k = match &self.kind {
            ProviderKind::Anthropic { .. } => "anthropic",
            ProviderKind::OpenAi { .. } => "openai",
            ProviderKind::Ollama { .. } => "ollama",
            ProviderKind::Mock => "mock",
        };
        format!("{k} · {}", self.model)
    }
}

/// Build the provider for a config. Native only for the HTTP kinds.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub fn build(config: &Config) -> Box<dyn crate::Provider> {
    match &config.kind {
        ProviderKind::Anthropic { api_key } => Box::new(crate::anthropic::Anthropic::new(
            api_key.clone(),
            config.model.clone(),
        )),
        ProviderKind::OpenAi { base_url, api_key } => Box::new(crate::openai::OpenAi::new(
            base_url.clone(),
            Some(api_key.clone()),
            config.model.clone(),
            config.embed_model.clone(),
        )),
        ProviderKind::Ollama { host } => Box::new(crate::openai::OpenAi::ollama(
            host.clone(),
            config.model.clone(),
            config.embed_model.clone(),
        )),
        ProviderKind::Mock => Box::new(crate::MockProvider::scripted()),
    }
}
