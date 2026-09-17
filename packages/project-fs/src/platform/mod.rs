//! Per-platform backends, selected by feature + target.
//! The rest of the crate talks to `trait Backend { read, write, list, stat, watch }`.

#[cfg(feature = "mobile")]
pub mod mobile;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod web;
