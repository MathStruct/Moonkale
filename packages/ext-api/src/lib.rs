//! # moonkale-ext-api
//!
//! **This crate is the contract.** Everything an extension can see or do is
//! declared here, and nothing else in the workspace is visible to an
//! extension. Its version is the extension API version; breaking it means a
//! major bump and a migration note in the vault.
//!
//! Moonkale is extension-driven: the built-in editors (`packages/editors/*`)
//! are themselves extensions that happen to be compiled in. There is no
//! privileged path — if the code editor can do it, a third-party extension
//! can do it.
//!
//! Two kinds of extension share this one API:
//!
//! | kind      | how it runs                            | where                    |
//! |-----------|----------------------------------------|--------------------------|
//! | `static`  | a Rust crate linked into the binary    | all platforms            |
//! | `wasm`    | a WASM *component* loaded at runtime   | desktop, server; web via |
//! |           | (`wasmtime` natively, `wasm_component_layer` on web) | the browser |
//!
//! A static extension implements [`Extension`] directly. A wasm extension
//! implements the same trait via bindings generated from `wit/moonkale.wit`
//! (the WIT world is *generated from* the Rust types in this crate so the two
//! can't drift).
//!
//! Extensions never get a raw `Source`, file handle or socket. They get a
//! [`host::Host`] handle whose methods are capability-checked against the
//! permissions declared in the [`manifest::Manifest`].
//!
//! See the vault: `markdown/extensions/Writing an Extension.md`.

pub mod capability;
pub mod contrib;
pub mod extension;
pub mod host;
pub mod manifest;
