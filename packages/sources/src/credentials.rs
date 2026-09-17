//! Secrets.
//!
//! | platform | store                                              |
//! |----------|----------------------------------------------------|
//! | desktop  | OS keychain via `keyring`; fallback encrypted file  |
//! | web      | never in the browser — the server holds them        |
//! | mobile   | iOS Keychain / Android Keystore via platform crate |
//!
//! `SecretRef` (a name) is what gets persisted in workspace config;
//! resolution to a `SecretString` happens at connect time.
