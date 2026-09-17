//! `trait Provider { complete(stream), embed, models() }` with
//! implementations for Anthropic, OpenAI-compatible endpoints, Ollama /
//! local servers. Provider config (base URL, model, key ref) is per
//! workspace; keys are `SecretRef`s resolved through
//! `moonkale-sources::credentials`.
