//! # moonkale-lsp
//!
//! The LSP *client* — everything that is not "how do I reach the server":
//!
//! - [`session`]: initialize / capabilities / shutdown state machine,
//! - [`sync`]: keep server documents in sync with the editor's rope via
//!   incremental `didChange`,
//! - [`features`]: map LSP features (hover, completion, go-to-def,
//!   diagnostics, semantic tokens, inlay hints) to editor requests,
//! - [`graph`]: feed definitions/references into `moonkale-index` as
//!   `Symbol` nodes and `References` edges — LSP is a second extractor
//!   alongside tree-sitter, more precise but slower and language-server
//!   dependent.
//!
//! Transport is a trait (`trait Transport: Stream<Msg> + Sink<Msg>`).
//! Spawning a local server is desktop-only and lives in
//! `moonkale-lsp-local`; on web/mobile the transport is a websocket to a
//! server that spawns it (`api`).

pub mod features;
pub mod graph;
pub mod session;
pub mod sync;
pub mod transport;
