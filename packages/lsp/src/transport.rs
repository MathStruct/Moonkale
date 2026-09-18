use futures_channel::mpsc;
use std::future::Future;
use std::pin::Pin;

/// A bidirectional stream of JSON-RPC messages (already unframed).
pub trait LspTransport {
    fn send(&self, message: String);
    /// The incoming message stream; `None` after the first call.
    fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<String>>;
}

pub type LspTransportFuture = Pin<Box<dyn Future<Output = Result<Box<dyn LspTransport>, String>>>>;
/// Installed by the platform: start (or connect to) a language server for
/// `(language id, workspace root)`.
pub type SpawnLsp = fn(String, String) -> LspTransportFuture;
