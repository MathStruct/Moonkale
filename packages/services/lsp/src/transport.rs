use futures_channel::mpsc;
use std::future::Future;
use std::pin::Pin;

/// Owned, object-safe transport closure future.
pub type LspCloseFuture = Pin<Box<dyn Future<Output = ()>>>;

/// A bidirectional stream of JSON-RPC messages (already unframed).
pub trait LspTransport {
    fn send(&self, message: String);
    /// The incoming message stream; `None` after the first call.
    fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<String>>;
    /// Release the underlying connection/process even if session handles remain.
    /// The compatibility default is a no-op; live transports should override it.
    fn close(&self) -> LspCloseFuture {
        Box::pin(async {})
    }
}

pub type LspTransportFuture = Pin<Box<dyn Future<Output = Result<Box<dyn LspTransport>, String>>>>;
/// Installed by the platform: start (or connect to) a language server for
/// `(language id, workspace root)`.
pub type SpawnLsp = fn(String, String) -> LspTransportFuture;
