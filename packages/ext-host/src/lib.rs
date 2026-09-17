//! # moonkale-ext-host
//!
//! Owns the lifecycle of extensions: discovery, manifest validation,
//! permission grants, lazy activation, dispatch, and isolation.
//!
//! ```text
//!   manifests ──► Registry ──► (activation event) ──► Runtime ──► Extension
//!                    │                                  ├─ static  (in-process)
//!                    │                                  ├─ wasmtime (native)
//!                    └─ contributions (panels,          └─ browser  (web)
//!                       commands, languages, …)
//!                       are available *before* activation
//! ```
//!
//! Platform notes:
//! - **desktop**: static + wasmtime. Extensions live in
//!   `~/.config/moonkale/extensions/<id>/`.
//! - **web**: static + browser runtime (`wasm_component_layer` over the
//!   page's own wasm engine). Extensions are fetched from the server that
//!   serves the app; no arbitrary URLs.
//! - **mobile**: static only in v1. Loading code at runtime is restricted by
//!   app-store policy on iOS; wasm-in-app is a grey area we avoid for now.
//! - **server** (the `api` crate): wasmtime, for extensions that contribute
//!   sources or LLM tools which must run where the database is reachable.

pub mod activation;
pub mod permissions;
pub mod registry;
pub mod runtime;
