//! The single error type shared across the model layer.
//!
//! Planned: `enum CoreError { NotFound(..), Conflict(..), Invalid(..),
//! Unsupported(..), Source(SourceError) }` via `thiserror`. Crates above
//! `core` wrap this in their own error types; crates below it don't exist.
//!
//! `Unsupported` is important: a `Source` is allowed to say "I cannot do
//! that" (e.g. a read-only DuckDB file cannot accept a write) and the UI must
//! render that gracefully rather than as a crash.
