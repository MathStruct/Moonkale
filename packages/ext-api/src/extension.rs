//! The `Extension` trait — the dynamic half.
//!
//! ```ignore
//! pub trait Extension: Send + Sync + 'static {
//!     /// Called once, lazily, when an activation event in the manifest fires.
//!     fn activate(&mut self, host: Host) -> Result<(), ExtError>;
//!     /// Called on shutdown / disable. Must be idempotent.
//!     fn deactivate(&mut self) {}
//!     /// Handle a command the extension registered. Args are validated
//!     /// against the manifest's schema before this is called.
//!     fn command(&mut self, id: &str, args: Value) -> Result<Value, ExtError>;
//!     /// Render a panel the extension contributed. Static extensions return
//!     /// `dioxus::Element`; wasm extensions return a `ui::Tree` (a small
//!     /// declarative UI description the shell renders — see host.rs).
//!     fn panel(&mut self, id: &str, ctx: PanelCtx) -> PanelOutput;
//! }
//! ```
//!
//! Static extensions register with `inventory`-style linking:
//! `moonkale_ext_api::register!(MyExt)`; the host collects them at startup.
//! WASM extensions export the same functions through the generated bindings.
