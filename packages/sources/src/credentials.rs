//! Secrets (design note). Desktop: OS keychain via `keyring`; web: never in the browser; mobile: Keychain/Keystore. `SecretRef` names are persisted, values resolved at connect time.
