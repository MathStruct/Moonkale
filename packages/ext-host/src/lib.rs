//! # moonkale-ext-host
//!
//! Third-party extensions as **wasm core modules** (Milestone 6, v1): a
//! module exports `manifest()` and `run(command_json)` and may import a few
//! host calls (`log`, `call`), all as JSON over linear memory. The host
//! checks the extension's *granted* permissions on every call. Extensions
//! are discovered in `~/.config/moonkale/extensions/*.wasm` and
//! `<folder>/.moonkale/extensions/*.wasm`; their commands become agent
//! tools (and, later, palette commands).
//!
//! ```text
//!   guest exports                 host imports (module "moonkale")
//!   ─────────────                 ───────────────────────────────
//!   alloc(len) -> ptr             log(ptr, len)
//!   manifest() -> packed(ptr,len) call(ptr, len) -> packed(ptr,len)   // JSON request → JSON reply
//!   run(ptr, len) -> packed(ptr,len)
//! ```
//!
//! `packed` is `(ptr << 32) | len` as `i64`. Everything is UTF-8 JSON.
//! Components + WIT ([[Extension System]]) remain the documented next step;
//! this ABI is small enough to be replaced by one without changing the
//! manifest format.
//!
//! Vault: `architecture/Extension System.md`, `extensions/Writing an Extension.md`.

pub mod abi;
#[cfg(all(feature = "wasmtime", not(target_arch = "wasm32")))]
pub mod runtime;

pub use abi::{HostCall, HostReply, RunReply, RunRequest, WasmCommand, WasmManifest, ABI_VERSION};
#[cfg(all(feature = "wasmtime", not(target_arch = "wasm32")))]
pub use runtime::{discover, Host, LoadedExtension, Runtime};
