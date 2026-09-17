//! Runtimes — how an extension's code actually executes.
//!
//! `trait Runtime { fn instantiate(&self, manifest) -> Box<dyn ExtensionInstance> }`
//! with three implementations selected by `cfg` + features. All three present
//! the same `ExtensionInstance` to the dispatcher, so nothing above this
//! module knows which one it is talking to.

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod browser_rt;
pub mod static_rt;
#[cfg(all(feature = "wasmtime", not(target_arch = "wasm32")))]
pub mod wasmtime_rt;
