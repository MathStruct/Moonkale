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
}
