//! `trait Provider`: streamed completions with tool use, and embeddings.
//! Implementations: [`crate::anthropic`], [`crate::openai`] (also Ollama's
//! `/v1`), [`crate::mock`], and `api::RemoteProvider` on the web client.

use crate::types::{Event, Request};
use futures_channel::mpsc::UnboundedReceiver;
use std::future::Future;
use std::pin::Pin;

/// Events arrive on a channel so the stream is `Send` regardless of the
/// provider's own future type.
pub type EventStream = UnboundedReceiver<Event>;

#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T>>>;

pub trait Provider: Send + Sync {
    /// "anthropic", "openai", "ollama", "mock", "remote".
    fn name(&self) -> String;
    /// The default chat model, for display.
    fn model(&self) -> String;
    /// Start a completion; events stream until `Done`/`Error`.
    fn complete(&self, request: Request) -> EventStream;
    /// Embeddings for `texts`, one vector each; `Err` if unsupported.
    fn embed(&self, texts: Vec<String>) -> BoxFuture<Result<Vec<Vec<f32>>, String>>;
    fn supports_embed(&self) -> bool {
        false
    }
    /// What the provider knows about its own readiness before a turn
    /// (Milestone 15): the `claude-code` provider reports the CLI's version
    /// and login; `None` = nothing to report (keyed HTTP providers, the
    /// web client's proxy).
    fn status(&self) -> BoxFuture<Option<ProviderStatus>> {
        Box::pin(async { None })
    }
}

/// A provider's readiness, for the settings panel.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderStatus {
    /// Ready to run a turn.
    pub ok: bool,
    /// One line: "claude 2.1.278 · logged in as x@y" / "not installed".
    pub summary: String,
    /// What to do about it, when not ok ("Log in", an install command).
    pub hint: Option<String>,
    /// `true` when logging in through the CLI would help (`claude auth login`).
    pub can_login: bool,
}
