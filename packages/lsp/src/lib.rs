//! # moonkale-lsp
//!
//! The LSP *client* — everything that is not "how do I reach the server":
//! JSON-RPC bookkeeping, the initialize handshake, document sync, and the
//! features the editor asks for (diagnostics, hover, go-to-definition).
//!
//! Transport is a trait ([`transport::LspTransport`]) carrying JSON-RPC
//! message *strings*; framing belongs to the transport. Implementations:
//! `moonkale-lsp-local::StdioTransport` (desktop/server) and
//! `api::RemoteLsp` (websocket, web). The session is single-threaded
//! (`Rc<RefCell<_>>`), so it runs unchanged inside the browser.
//!
//! Milestone 3: full-document sync, diagnostics, hover, definition.
//! Milestone 7: completion, rename, code actions, references
//! (`WorkspaceEdit` applied by the editor extension). The index integration
//! (`lsp::graph` in the design) is still to come.

pub mod session;
pub mod transport;

pub use lsp_types;
pub use session::{
    char_offset, strip_snippet, CodeAction, CompletionItem, Diagnostic, Location, LspEvent,
    LspSession, TextEdit, WorkspaceEdit,
};
pub use transport::{LspTransport, LspTransportFuture, SpawnLsp};
