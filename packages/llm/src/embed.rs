//! Embedding requests with batching, caching by content hash, and model
//! versioning so the index knows when vectors are stale. Local models
//! (via an Ollama-style server or a future in-process runtime) are just
//! another `Provider`.
