//! Permission store + prompt flow.
//!
//! Grants are persisted per (extension id, capability). First use of a
//! capability prompts the user through `Host::ask`; org policies (a TOML
//! file / server config) can pre-grant or hard-deny. The `Host` handed to
//! each extension is a thin wrapper that consults this store on every call.
